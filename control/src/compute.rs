use defmt::*;
use embassy_time::Instant;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use heapless::Vec;
use libm::{sinf, powf, sqrtf};

use crate::{
    control::{ComputeReqMsg, ComputeResMsg}, math::{self, quicksort_xy, LsScratch}, storage::{BinStorage, FlashStorage, MeasurementStorage}, types::{Config, Measurement, Observation, Sector, BIN_BURST_SIZE, BURST_SIZE}, StorageType
};

const QC_MIN_SAMPLES: u32 = 1000;
const QC_MIN_MAX_AMP: f32 = 500.0;
const QC_MIN_PEAK_TO_MEAN: f32 = 3.0;

const ARC_GAP: u16 = 120;
const C_M_S: f32 = 299_792_458.0;
const BUF_BYTES: usize = BIN_BURST_SIZE * BURST_SIZE * 1;

const MIN_HEIGHT: f32 = 2.0;
const MAX_HEIGHT: f32 = 7.0;
const STEP_SIZE: f32 = 0.05;

const MAX_BINS: usize = 12;

type ArcQueue = Vec<Arc, 256>; // 6 * 256 = 1536 bytes
type ObservationVec = Vec<Observation, 256>; // 24 * 256 = 6144 bytes
type RangeVec = Vec<f32, 512>; // 4 * 512 = 1024 bytes
type AmplVec = Vec<f32, 512>; // 4 * 512 = 1024 bytes
type SampleVec = Vec<f32, { BIN_BURST_SIZE * MAX_BINS }>; // 4 * 240 * 12 = 11520 bytes

#[derive(Debug, Clone, Copy)]
struct Arc {
    id: u16,
    start_time: u16,
    end_time: u16,
}

#[derive(Clone, Copy)]
pub struct Record {
    data: u32,
}

impl Record {
    // format ABCDEF
    // A - 7 bit satellite ID - 0x7F
    // B - 2 bit network - 0x3
    // C - 7 bit elevation (0-90) - 0x7F
    // D - 9 bit azimuth (0-359) - 0x1FF
    // E - 6 bit SNR (0-64) - 0x3F
    // F - 1 bit band (0=L1, 1=L5) - 0x1
    #[inline]
    pub fn from_sample(sample: u32) -> Self {
        Self { data: sample }
    }

    #[inline]
    pub fn get_id(&self) -> u16 {
        (1 + self.get_network() as u16) * 10000
            + self.get_band() as u16 * 1000
            + self.get_satellite() as u16
    }

    #[inline]
    pub fn get_band(&self) -> bool {
        (self.data & 0x1) != 0
    }

    #[inline]
    pub fn get_snr(&self) -> u8 {
        ((self.data >> 1) & 0x3F) as u8
    }

    #[inline]
    pub fn get_azimuth(&self) -> u16 {
        ((self.data >> 7) & 0x1FF) as u16
    }

    #[inline]
    pub fn get_elevation(&self) -> u8 {
        ((self.data >> 16) & 0x7F) as u8
    }

    #[inline]
    pub fn get_network(&self) -> u8 {
        ((self.data >> 23) & 0x3) as u8
    }

    #[inline]
    pub fn get_satellite(&self) -> u8 {
        ((self.data >> 25) & 0x7F) as u8
    }
}

#[embassy_executor::task]
pub async fn task_compute(
    channel_req: &'static Channel<CriticalSectionRawMutex, ComputeReqMsg, 8>,
    channel_res: &'static Channel<CriticalSectionRawMutex, ComputeResMsg, 8>,
    storage: &'static StorageType,
) {
    loop {
        info!("[comp] waiting for request");
        let message = channel_req.receive().await;
        match message {
            ComputeReqMsg::Compute { sector, config } => {
                info!("[comp] starting computation for sector {}", sector.get_index());
                run_compute(sector, storage, config).await;
                channel_res.send(ComputeResMsg::Success).await;
            }
        }
    }
}

async fn run_compute(
    sector: Sector, 
    storage: &'static StorageType,
    config: Config,
) {
    // One reusable IO buffer for the whole task (no per-call stack duplication).
    let total_start = Instant::now();
    let bin_storage = BinStorage::new(config.seconds_per_bin);
    let measurement_storage = MeasurementStorage::new();
    let mut io_buf = [0u8; BUF_BYTES]; // 24 * 64 * 4 = 61440 bytes

    let queue: ArcQueue; // 1536 bytes

    let start = Instant::now();
    {
        let mut storage_lock = storage.lock().await;
        let storage = storage_lock.as_mut().expect("Storage not initialized");
        queue = build_arc_queue(&sector, &bin_storage, storage, &mut io_buf);
    }
    info!(
        "[comp] created queue with {} arcs in {} ms",
        queue.len(),
        (Instant::now() - start).as_millis()
    );

    let start = Instant::now();
    let (range, size) = lin_range(MIN_HEIGHT, MAX_HEIGHT, STEP_SIZE);
    info!(
        "[comp] generated linear range with {} steps in {} ms",
        size,
        (Instant::now() - start).as_millis()
    );

    let mut observations: ObservationVec = Vec::new();

    // Reuse these buffers for every arc to avoid per-arc allocations.
    let mut times: SampleVec = Vec::new(); // 38400 bytes
    let mut elevs: SampleVec = Vec::new(); // 38400 bytes
    let mut snrs: SampleVec = Vec::new(); // 38400 bytes
    let mut ampls: AmplVec = Vec::new(); // 1024 bytes

    let total_arcs: usize = queue.len(); 

    for (idx, arc) in queue.iter().enumerate() {
        let full_start = Instant::now();
        info!(
            "[comp][{:03}/{:03}] arc sat {}, {}..{}",
            idx,
            total_arcs,
            arc.id,
            arc.start_time,
            arc.end_time
        );

        // Clear buffers for new arc.
        times.clear();
        elevs.clear();
        snrs.clear();
        ampls.clear();

        // Stream through storage and collect all records for this arc.
        let start = Instant::now();
        let (num_records, first_net_band);
        {
            let mut storage_lock = storage.lock().await;
            let storage = storage_lock.as_mut().expect("Storage not initialized");
            (num_records, first_net_band) = collect_arc_records(
                &sector,
                &bin_storage,
                storage,
                &mut io_buf,
                *arc,
                &mut times,
                &mut elevs,
                &mut snrs,
            );
        }
        info!(
            "[comp][{:03}/{:03}] fetched {} records in {} ms",
            idx,
            total_arcs,
            num_records,
            (Instant::now() - start).as_millis()
        );

        // if num_records < QC_MIN_SAMPLES {
        //     info!("[comp][{:03}/{:03}] insufficient records, skip", idx, total_arcs);
        //     continue;
        // }

        // Make sure elevation is smooth over time
        let start = Instant::now();
        math::polyfit_and_smooth_no_std(&times, &mut elevs);
        info!(
            "[comp][{:03}/{:03}] smoothed elevation in {} ms",
            idx,
            total_arcs,
            (Instant::now() - start).as_millis()
        );

        // Compute the relevant wavelength
        let (net, band) = first_net_band.unwrap_or((0, false));
        let (freq_hz, cf) = compute_cf(net, band);
        info!(
            "[comp][{:03}/{:03}] freq {} MHz, cf {} (net {}, band {})",
            idx,
            total_arcs,
            freq_hz / 1_000_000.0,
            cf,
            net,
            band
        );

        // Transform samples: x = sin(e)/cf, y = snr.
        let start = Instant::now();
        transform_xy(&mut elevs, cf);
        info!(
            "[comp][{:03}/{:03}] computed {} transformed samples in {} ms",
            idx,
            total_arcs,
            elevs.len(),
            (Instant::now() - start).as_millis()
        );

        // In-place sort x and y together (no extra memory).
        let start = Instant::now();
        quicksort_xy(&mut elevs, &mut snrs);
        info!(
            "[comp][{:03}/{:03}] sorted {} pairs in {} ms",
            idx,
            total_arcs,
            elevs.len(),
            (Instant::now() - start).as_millis()
        );

        for _ in 0..size {
            ampls.push(0.0).ok();
        }
       

        let start = Instant::now();
        math::lombscargle_no_std::<{ BURST_SIZE * MAX_BINS }>(&elevs, &snrs, MIN_HEIGHT, STEP_SIZE, size, &mut ampls);
        info!(
            "[comp][{:03}/{:03}] Lomb-Scargle in {} ms",
            idx,
            total_arcs,
            (Instant::now() - start).as_millis()
        );

        if let Some((max_amp, max_rh, mean_amp)) = ampl_stats(&range, &ampls) {
            let _ = observations.push(Observation {
                sat_id: arc.id,
                start_time: arc.start_time,
                end_time: arc.end_time,
                max_amp,
                max_rh,
                mean_amp,
                num_recs: num_records,
                used: false,
            });
        } else {
            info!("[comp][{:03}/{:03}] no valid amplitude values, skipping", idx, total_arcs);
        }

        info!(
            "[comp][{:03}/{:03}] finished arc in {} ms",
            idx,
            total_arcs,
            (Instant::now() - full_start).as_millis()
        );
    }

    let mut rh_sum: f32 = 0.0;
    let mut used_count: u32 = 0;

    for observation in &mut observations {
        if observation.max_amp < QC_MIN_MAX_AMP {
            info!(
                "[comp] sat {} ({}..{}) - max_amp {} too low, skipping",
                observation.sat_id, observation.start_time, observation.end_time, observation.max_amp
            );
            continue;
        }
        else if observation.peak_to_mean() < QC_MIN_PEAK_TO_MEAN {
            info!(
                "[comp] sat {} ({}..{}) - peak/mean {} too low, skipping",
                observation.sat_id, observation.start_time, observation.end_time, observation.peak_to_mean()
            );
            continue;
        }

        info!(
            "[comp] sat {} ({}..{}) - max_amp {}, max_rh {}, mean_amp {}, peak/mean {}, num_recs {}",
            observation.sat_id, observation.start_time, observation.end_time,
            observation.max_amp, observation.max_rh, observation.mean_amp,
            observation.peak_to_mean(), observation.num_recs
        );
        observation.used = true;
        rh_sum += observation.max_rh;
        used_count += 1;
    }

    let rh_mean = rh_sum / used_count as f32;
    let mut rh_std_sum: f32 = 0.0;

    for observation in &observations {
        if !observation.used {
            continue;
        }
        rh_std_sum += powf(observation.max_rh - rh_mean, 2.0);
    }

    let rh_std = sqrtf(rh_std_sum / used_count as f32);

    let mut measurement = Measurement::new(
        queue.len() as u32, 
        sector.get_start_time(), 
        sector.get_end_time(), 
        rh_mean, 
        rh_std
    );

    for observation in observations {
        measurement.push(observation);
    }

    {
        let mut storage_lock = storage.lock().await;
        let storage = storage_lock.as_mut().expect("Storage not initialized");
        measurement_storage.store(storage, sector.get_index(), measurement);
    }

    if used_count > 0 {
        info!(
            "[comp] overall mean rh: {} (std {}, samples {}) in {} ms",
            rh_mean,
            rh_std,
            used_count,
            (Instant::now() - total_start).as_millis()
        );
    } else {
        info!(
            "[comp] no valid observations in {} ms",
            (Instant::now() - total_start).as_millis()
        );
        return;
    }
}

#[inline]
fn compute_cf(net: u8, band: bool) -> (f32, f32) {
    let freq_hz = network_band_to_frequency(net, band);
    let wavelength = C_M_S / freq_hz;
    let cf = wavelength / 2.0;
    (freq_hz, cf)
}

#[inline]
fn network_band_to_frequency(network: u8, band: bool) -> f32 {
    match network {
        0 => if !band { 1575.42e6 } else { 1176.45e6 },
        1 => if !band { 1602.00e6 } else { 1602.00e6 },
        2 => if !band { 1575.42e6 } else { 1176.45e6 },
        3 => if !band { 1561.098e6 } else { 1176.45e6 },
        _ => 1.0,
    }
}

#[inline]
fn lin_range(start: f32, end: f32, step_size: f32) -> (Vec<f32, 512>, usize) {
    let mut result = Vec::<f32, 512>::new();
    let mut current = start;
    let mut count = 0;

    while current <= end {
        let _ = result.push(current);
        current += step_size;
        count += 1;
    }

    (result, count)
}

#[inline]
fn transform_xy(elevs: &mut SampleVec, cf: f32) {
    let inv_cf = 1.0 / cf;
    for i in 0..elevs.len() {
        let s = sinf(elevs[i].to_radians()) * inv_cf;
        elevs[i] = s;
    }
}

#[inline]
fn ampl_stats(range: &RangeVec, ampl: &AmplVec) -> Option<(f32, f32, f32)> {
    let mut max = 0.0f32;
    let mut max_idx = 0usize;
    let mut mean_acc = 0.0f64;
    let mut count: u32 = 0;

    for (i, &p) in ampl.iter().enumerate() {
        if p.is_finite() && p > 0.0 {
            if p > max {
                max = p;
                max_idx = i;
            }
            mean_acc += p as f64;
            count = count.saturating_add(1);
        }
    }
    if count == 0 {
        None
    } else {
        let mean = (mean_acc / count as f64) as f32;
        Some((max, range[max_idx], mean))
    }
}

/// Build the arc queue by streaming records directly out of storage.
/// Avoids building any `Burst` or `u32` buffers beyond `io_buf`.
fn build_arc_queue(
    sector: &Sector,
    bin_storage: &BinStorage,
    storage: &mut FlashStorage,
    io_buf: &mut [u8; BUF_BYTES],
) -> ArcQueue {
    let mut queue: ArcQueue = Vec::new();

    info!("[comp] building arc queue for sector {}", sector.get_index());

    for_each_record_in_sector(sector, bin_storage, storage, io_buf, |time, rec| {
        let id = rec.get_id();

        // Find most recent arc for this id (search from end).
        if let Some(arc) = queue.iter_mut().rev().find(|x| x.id == id) {
            if time > arc.start_time {
                if time - arc.end_time > ARC_GAP {
                    queue.push(Arc { id, start_time: time, end_time: time }).ok();
                } else {
                    arc.end_time = time;
                }
            }
        } else {
            queue.push(Arc { id, start_time: time, end_time: time }).ok();
        }
    });

    queue
}

/// Stream through sector and fill `times`, `elevs`, `snrs` for one arc.
/// Returns (num_records_seen_for_arc, Some((net, band)) from first match).
fn collect_arc_records(
    sector: &Sector,
    bin_storage: &BinStorage,
    storage: &mut FlashStorage,
    io_buf: &mut [u8; BUF_BYTES],
    arc: Arc,
    times: &mut SampleVec,
    elevs: &mut SampleVec,
    snrs: &mut SampleVec,
) -> (u32, Option<(u8, bool)>) {
    let mut num: u32 = 0;
    let mut first: Option<(u8, bool)> = None;

    for_each_record_in_sector(sector, bin_storage, storage, io_buf, |time, rec| {
        if rec.get_id() == arc.id && time >= arc.start_time && time <= arc.end_time {
            if first.is_none() {
                first = Some((rec.get_network(), rec.get_band()));
            }
            num = num.saturating_add(1);
            times.push(time as f32).ok();
            elevs.push(rec.get_elevation() as f32).ok();
            snrs.push(rec.get_snr() as f32).ok();
        }
    });

    (num, first)
}

/// Core streaming reader: iterates every (time, Record) in all bins without allocating.
/// Calls `f(time, record)` for each record.
fn for_each_record_in_sector<F>(
    sector: &Sector,
    bin_storage: &BinStorage,
    storage: &mut FlashStorage,
    io_buf: &mut [u8; BUF_BYTES],
    mut f: F,
) where
    F: FnMut(u16, Record),
{
    let bins = sector.get_bins();

    for &bin_id in bins.iter() {
        match bin_storage.read(storage, bin_id, io_buf) {
            Ok(_) => {
                // Interpret as stream of little-endian u32 words.
                let mut words = io_buf.chunks_exact(4);

                // Each "burst" begins with a header word, then `num` sample words.
                while let Some(hdr_b) = words.next() {
                    let header = u32::from_le_bytes([hdr_b[0], hdr_b[1], hdr_b[2], hdr_b[3]]);
                    let time = (header >> 8) as u16;
                    let num = (header & 0xFF) as u8;

                    if time == u16::MAX || num == 0 {
                        continue;
                    }

                    for _ in 0..num {
                        if let Some(smp_b) = words.next() {
                            let sample =
                                u32::from_le_bytes([smp_b[0], smp_b[1], smp_b[2], smp_b[3]]);
                            f(time, Record::from_sample(sample));
                        } else {
                            // Truncated burst — stop gracefully.
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                info!("Error reading bin {}: {:?}", bin_id, e);
            }
        }
    }
}
