use defmt::info;
use embassy_time::Timer;

use crate::{compute::{Record, BUF_BYTES}, storage::{BinStorage, MeasurementStorage, SectorStorage}, types::{CONTAINER_SIZE, NUM_BINS, NUM_MEASUREMENTS}, StorageType};

pub async fn dump(storage: &'static StorageType) {
    let mut storage_lock = storage.lock().await;
    let storage = storage_lock.as_mut().expect("Storage should be initialized");

    let sector_storage = SectorStorage::new();
    let sectors = sector_storage.load(storage, false);
    if let Ok(sectors) = sectors {
        info!("SECTOR:idx, uid, state, midpoint_idx, measurement_idx, start_bin_idx, start_time, lat, lon");
        for (i, sector) in sectors.iter().enumerate() {
            info!(
                "SECTOR:{}, {}, {}, {}, {}, {}, {}, {}, {}", 
                i,
                sector.get_uid(),
                sector.state,
                sector.get_midpoint_index(),
                sector.get_measurement_index(),
                sector.get_start_bin_index(),
                sector.get_start_time(),
                sector.get_lat(),
                sector.get_lon(),
            );
        }
    }

    info!("MEASUREMENT:idx, uid, mean, std, num_seen, start_time, end_time, lat, lon");
    info!("OBSERVATION:meas_idx, obs_idx, sat_id, start_time, end_time, used, max_rh, max_amp, mean_amp, num_recs");
    let measurement_storage = MeasurementStorage::new();
    for i in 0..NUM_MEASUREMENTS {
        if let Some(measurement) = measurement_storage.read(storage, i as u32) {
            info!("MEASUREMENT:{}, {}, {}, {}, {}, {}, {}, {}, {}",
                i,
                measurement.uid,
                measurement.mean,
                measurement.std,
                measurement.num_seen,
                measurement.start_time,
                measurement.end_time,
                measurement.lat,
                measurement.lon
            );
            for (j, observation) in measurement.observations.iter().enumerate() {
                info!("OBSERVATION:{}, {}, {}, {}, {}, {}, {}, {}, {}, {}",
                    i,
                    j,
                    observation.sat_id,
                    observation.start_time,
                    observation.end_time,
                    observation.used,
                    observation.max_rh,
                    observation.max_amp,
                    observation.mean_amp,
                    observation.num_recs
                );

                Timer::after_millis(1).await;
            }
        }
    }

    info!("DATA:bin_idx,time,id,satellite,network,band,elevation,azimuth,snr");
    let bin_storage = BinStorage::new();

    for i in 0..NUM_BINS {
        let mut buffer = [0u8; CONTAINER_SIZE];
        let result = bin_storage.read(storage, i as u32, &mut buffer);
        if let Ok(_) = result {
            let mut words = buffer.chunks_exact(4);

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

                        info!("DATA:{},{},{},{},{},{},{},{},{}",
                            i,
                            time,
                            sample.get_id(),
                            sample.get_satellite(),
                            sample.get_network(),
                            sample.get_band(),
                            sample.get_elevation(),
                            sample.get_azimuth(),
                            sample.get_snr()
                        );
                    } else {
                        break;
                    }
                }

                Timer::after_millis(1).await;
            }
        } else {
            info!("[main] no data for bin {}: {}", i, result.err().unwrap());
        }
    }
}