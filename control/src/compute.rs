use defmt::*;
use embassy_futures::yield_now;
use embassy_time::Instant;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use heapless::Vec;
use libm::{powf, sinf, sqrtf};

use crate::StorageType;
use crate::storage::{FlashStorage, BinStorage, MeasurementStorage};
use crate::types::{Config, Sector, BIN_BURST_SIZE, BURST_SIZE};
use crate::messages::{ComputeReqMsg, ComputeResMsg};
use crate::types::{Measurement, Observation};
use crate::math::{detrend_no_std, lombscargle_no_std, polyfit_and_smooth_no_std, quicksort_xy};

const ARC_GAP: u16 = 120;
const C_M_S: f32 = 299_792_458.0;
pub const BUF_BYTES: usize = BIN_BURST_SIZE * BURST_SIZE * 1;

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

#[derive(Debug, Format)]
pub enum ComputeError {
    ObservationOverflow,
    StorageAccess,
    StorageWrite,
}

#[embassy_executor::task]
pub async fn task_compute(
    channel_req: &'static Channel<CriticalSectionRawMutex, ComputeReqMsg, 8>,
    channel_res: &'static Channel<CriticalSectionRawMutex, ComputeResMsg, 8>,
    storage: &'static StorageType,
) {
    info!("[comp] starting");
    loop {
        info!("[comp] waiting for request...");
        let message = channel_req.receive().await;
        match message {
            ComputeReqMsg::Compute { sector, config } => {
                info!("[comp] starting computation for sector {}", sector.get_measurement_index());
                match run_compute(&sector, storage, &config).await {
                    Ok(()) => {
                        channel_res.send(ComputeResMsg::Success { sector_uid: sector.get_uid() }).await;
                    },
                    Err(e) => {
                        channel_res.send(ComputeResMsg::ComputeFail { sector_uid: sector.get_uid(), error: e }).await;
                    }
                }
            }
        }
    }
}

async fn run_compute(
    sector: &Sector, 
    storage: &'static StorageType,
    config: &Config,
) -> Result<(), ComputeError> {
    // One reusable IO buffer for the whole task (no per-call stack duplication).
    let total_start = Instant::now();
    let bin_storage = BinStorage::new();
    let measurement_storage = MeasurementStorage::new();
    let mut io_buf = [0u8; BUF_BYTES]; // 24 * 64 * 4 = 61440 bytes

    let queue: ArcQueue; // 1536 bytes

    let start = Instant::now();
    {
        let mut storage_lock = storage.lock().await;
        let storage = storage_lock.as_mut().ok_or(ComputeError::StorageAccess)?;
        queue = build_arc_queue(&sector, &bin_storage, storage, &mut io_buf, &config)?;
    }
    info!(
        "[comp] created queue with {} arcs in {} ms",
        queue.len(),
        (Instant::now() - start).as_millis()
    );

    let start = Instant::now();
    let (range, size) = lin_range(config.min_relative_height, config.max_relative_height, config.relative_height_step_size);
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
        if idx >= 256 {
            info!("[comp] reached max arcs to process (256), stopping");
            break;
        }

        let full_start = Instant::now();
        info!(
            "[comp][{:03}/{:03}] arc sat {}, {}..{}",
            idx,
            total_arcs-1,
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
            let storage = storage_lock.as_mut().ok_or(ComputeError::StorageAccess)?;
            (num_records, first_net_band) = collect_arc_records(
                &sector,
                &bin_storage,
                storage,
                &mut io_buf,
                *arc,
                &mut times,
                &mut elevs,
                &mut snrs,
                &config
            )?;
        }
        info!(
            "[comp][{:03}/{:03}] fetched {} records in {} ms",
            idx,
            total_arcs-1,
            num_records,
            (Instant::now() - start).as_millis()
        );

        let min_elev = elevs.iter().fold(f32::INFINITY, |a, &b| libm::fminf(a, b));
        let max_elev = elevs.iter().fold(f32::NEG_INFINITY, |a, &b| libm::fmaxf(a, b));

        if max_elev - min_elev < config.qc_min_elevation_range as f32 {
            info!(
                "[comp][{:03}/{:03}] elevation range {} too small, skipping",
                idx,
                total_arcs-1,
                max_elev - min_elev
            );
            yield_now().await;
            continue;
        }

        // Make sure elevation is smooth over time
        let start = Instant::now();
        polyfit_and_smooth_no_std(&times, &mut elevs);
        info!(
            "[comp][{:03}/{:03}] smoothed elevation in {} ms",
            idx,
            total_arcs-1,
            (Instant::now() - start).as_millis()
        );

        // Compute the relevant wavelength
        let (net, band) = first_net_band.unwrap_or((0, false));
        let (freq_hz, cf) = compute_cf(net, band);
        info!(
            "[comp][{:03}/{:03}] freq {} MHz, cf {} (net {}, band {})",
            idx,
            total_arcs-1,
            freq_hz / 1_000_000.0,
            cf,
            net,
            band
        );

        // Transform elevation e = sin(e)/cf,
        let start = Instant::now();
        transform_elevs(&mut elevs, cf);
        info!(
            "[comp][{:03}/{:03}] computed {} transformed elevations in {} ms",
            idx,
            total_arcs-1,
            elevs.len(),
            (Instant::now() - start).as_millis()
        );

        // Transform snrs snr = 10^(snr/20)
        let start = Instant::now();
        transform_snrs(&mut snrs);
        info!(
            "[comp][{:03}/{:03}] computed {} transformed snrs in {} ms",
            idx,
            total_arcs-1,
            snrs.len(),
            (Instant::now() - start).as_millis()
        );

        // Detrend the SNR
        let start = Instant::now();
        detrend_no_std(&elevs, &mut snrs);
        info!(
            "[comp][{:03}/{:03}] detrended SNR in {} ms",
            idx,
            total_arcs-1,
            (Instant::now() - start).as_millis()
        );

        // In-place sort x and y together (no extra memory).
        let start = Instant::now();
        quicksort_xy(&mut elevs, &mut snrs);
        info!(
            "[comp][{:03}/{:03}] sorted {} pairs in {} ms",
            idx,
            total_arcs-1,
            elevs.len(),
            (Instant::now() - start).as_millis()
        );

        for _ in 0..size {
            ampls.push(0.0).ok();
        }

        let start = Instant::now();
        lombscargle_no_std::<{ BIN_BURST_SIZE * MAX_BINS }>(&elevs, &snrs, config.min_relative_height, config.relative_height_step_size, size, &mut ampls).await;
        info!(
            "[comp][{:03}/{:03}] Lomb-Scargle in {} ms",
            idx,
            total_arcs-1,
            (Instant::now() - start).as_millis()
        );

        if let Some((max_amp, max_rh, max_amp_2, max_rh_2, mean_amp)) = ampl_stats(&range, &mut ampls, num_records) {
            if max_amp / max_amp_2 < config.qc_min_peak_to_peak {
                info!(
                    "[comp][{:03}/{:03}] peak to peak ratio {} below threshold {}, skipping",
                    idx,
                    total_arcs-1,
                    max_amp / max_amp_2,
                    config.qc_min_peak_to_peak
                );
                yield_now().await;
                continue;
            }

            let observation = Observation {
                sat_id: arc.id,
                start_time: arc.start_time,
                end_time: arc.end_time,
                max_amp,
                max_rh,
                max_amp_2,
                max_rh_2,
                mean_amp,
                num_recs: num_records,
                used: false,
            };

            info!(
                "[comp][{:03}/{:03}] sat {} ({}..{}) - max_amp {}, max_rh {}, mean_amp {}, peak/mean {}, num_recs {} in {} ms",
                idx,
                total_arcs-1,
                observation.sat_id, 
                observation.start_time, 
                observation.end_time,
                observation.max_amp, 
                observation.max_rh, 
                observation.mean_amp,
                observation.peak_to_mean(),
                observation.num_recs,
                (Instant::now() - full_start).as_millis()
            );

            observations.push(observation).map_err(|_| ComputeError::ObservationOverflow)?;
        } else {
            info!("[comp][{:03}/{:03}] no valid amplitude values, skipping after {} ms", idx, total_arcs-1, (Instant::now() - full_start).as_millis());
        }

        yield_now().await;
    }

    // Calculate IQR
    let n_observations = observations.len();
    let (min_bound, max_bound) = if n_observations >= 4 {
        // Sort rh_values
        observations.sort_unstable_by(|a, b| a.max_rh.partial_cmp(&b.max_rh).unwrap_or(core::cmp::Ordering::Equal));
        let q1_idx = n_observations / 4;
        let q3_idx = 3 * n_observations / 4;
        let q1 = observations[q1_idx].max_rh;
        let q3 = observations[q3_idx].max_rh;
        let iqr = q3 - q1;
        let min_bound = q1 - config.qc_iqr_size * iqr;
        let max_bound = q3 + config.qc_iqr_size * iqr;
        (min_bound, max_bound)
    } else {
        (0.0, 100.0)
    };

    info!("[comp] QC bounds: min {}, max {}", min_bound, max_bound);

    let mut num_used = 0;
    let mut rh_sum: f32 = 0.0;

    for obs in observations.iter_mut() {
        if obs.max_rh < min_bound || obs.max_rh > max_bound {
            continue;
        }

        obs.used = true;
        num_used += 1;
        rh_sum += obs.max_rh;
    }

    let rh_mean = if num_used > 0 { rh_sum / num_used as f32 } else { 0.0 };
    let mut rh_var_acc: f32 = 0.0;

    for obs in observations.iter() {
        if obs.used {
            let diff = obs.max_rh - rh_mean;
            rh_var_acc += diff * diff;
        }
    }

    let rh_std = if num_used > 1 { sqrtf(rh_var_acc / (num_used as f32 - 1.0)) } else { 0.0 };

    let mut measurement = Measurement::new(
        sector.get_uid(),
        queue.len() as u32, 
        sector.get_start_time(), 
        sector.get_end_time(), 
        rh_mean, 
        rh_std,
        sector.get_lat(),
        sector.get_lon(),
    );

    for observation in observations {
        measurement.push(observation);
    }

    {
        let mut storage_lock = storage.lock().await;
        let storage = storage_lock.as_mut().ok_or(ComputeError::StorageAccess)?;
        measurement_storage.store(storage, sector.get_measurement_index(), measurement).map_err(|_| ComputeError::StorageWrite)?;
    }

    if num_used > 0 {
        info!(
            "[comp] overall mean rh: {} (std {}, samples {}) in {} ms",
            rh_mean,
            rh_std,
            num_used,
            (Instant::now() - total_start).as_millis()
        );
    } else {
        info!(
            "[comp] no valid observations in {} ms",
            (Instant::now() - total_start).as_millis()
        );
    }

    Ok(())
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
        1 => if !band { 1603.69e6 } else { 1603.69e6 },
        2 => if !band { 1575.42e6 } else { 1176.45e6 },
        3 => if !band { 1561.098e6 } else { 1176.45e6 },
        4 => if !band { 1575.42e6 } else { 1176.45e6 },
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
fn transform_elevs(elevs: &mut SampleVec, cf: f32) {
    let inv_cf = 1.0 / cf;
    for i in 0..elevs.len() {
        let s = sinf(elevs[i].to_radians()) * inv_cf;
        elevs[i] = s;
    }
}

#[inline]
fn transform_snrs(snrs: &mut SampleVec) {
    for i in 0..snrs.len() {
        snrs[i] = powf(10.0, snrs[i] / 20.0);
    }
}

#[inline]
fn ampl_stats(range: &RangeVec, ampl: &mut AmplVec, length: u32) -> Option<(f32, f32, f32, f32, f32)> {
    let mut max = 0.0f32;
    let mut max_idx = 0usize;
    let mut mean_acc = 0.0f64;
    let mut count: u32 = 0;

    let length_f = if length == 0 { 1.0 } else { length as f32 };

    for p in ampl.iter_mut() {
        if p.is_finite() && *p > 0.0 {
            *p = sqrtf(*p / length_f) * 2.0;
        } else {
            *p = 0.0;
        }
    }

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

    let mut second_peak_idx: Option<usize> = None;
    let mut second_peak_val: f32 = 0.0;
    if ampl.len() >= 3 {
        for i in 1..(ampl.len() - 1) {
            if i == max_idx {
                continue;
            }
            let v = ampl[i];
            if v.is_finite() && v > 0.0 && ampl[i - 1] < v && ampl[i + 1] < v {
                if second_peak_idx.is_none() || v > second_peak_val {
                    second_peak_idx = Some(i);
                    second_peak_val = v;
                }
            }
        }
    }

    if count == 0 {
        None
    } else {
        let mean = (mean_acc / count as f64) as f32;
        Some((max, range[max_idx], second_peak_val, range[second_peak_idx.unwrap_or(0)], mean))
    }
}

/// Build the arc queue by streaming records directly out of storage.
/// Avoids building any `Burst` or `u32` buffers beyond `io_buf`.
fn build_arc_queue(
    sector: &Sector,
    bin_storage: &BinStorage,
    storage: &mut FlashStorage,
    io_buf: &mut [u8; BUF_BYTES],
    config: &Config,
) -> Result<ArcQueue, ComputeError> {
    let mut queue: ArcQueue = Vec::new();

    info!("[comp] building arc queue for sector {}", sector.get_measurement_index());

    for_each_record_in_sector(sector, bin_storage, storage, io_buf, config, |time, rec| {
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
    })?;

    Ok(queue)
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
    config: &Config
) -> Result<(u32, Option<(u8, bool)>), ComputeError> {
    let mut num: u32 = 0;
    let mut first: Option<(u8, bool)> = None;

    for_each_record_in_sector(sector, bin_storage, storage, io_buf, config, |time, rec| {
        if rec.get_id() == arc.id && time >= arc.start_time && time <= arc.end_time {
            if first.is_none() {
                first = Some((rec.get_network(), rec.get_band()));
            }
            num = num.saturating_add(1);
            times.push(time as f32).ok();
            elevs.push(rec.get_elevation() as f32).ok();
            snrs.push(rec.get_snr() as f32).ok();
        }
    })?;

    Ok((num, first))
}

/// Core streaming reader: iterates every (time, Record) in all bins without allocating.
/// Calls `f(time, record)` for each record.
fn for_each_record_in_sector<F>(
    sector: &Sector,
    bin_storage: &BinStorage,
    storage: &mut FlashStorage,
    io_buf: &mut [u8; BUF_BYTES],
    config: &Config,
    mut f: F,
) -> Result<(), ComputeError>
where
    F: FnMut(u16, Record)
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
                            let sample = Record::from_sample(u32::from_le_bytes([smp_b[0], smp_b[1], smp_b[2], smp_b[3]]));
                            
                            if sample.get_elevation() < config.post_min_elevation as u8
                                || sample.get_elevation() > config.post_max_elevation as u8
                                || (
                                    config.post_min_azimuth > config.post_max_azimuth
                                        && (sample.get_azimuth() < config.post_min_azimuth as u16
                                            || sample.get_azimuth() > config.post_max_azimuth as u16)
                                )
                                || (
                                    config.post_min_azimuth <= config.post_max_azimuth
                                        && (sample.get_azimuth() < config.post_min_azimuth as u16
                                            || sample.get_azimuth() > config.post_max_azimuth as u16)
                                )
                            {
                                continue;
                            }
                            f(time, sample);
                        } else {
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                info!("[comp] Error reading bin {}: {:?}", bin_id, e);
                return Err(ComputeError::StorageAccess);
            }
        }
    }

    Ok(())
}
