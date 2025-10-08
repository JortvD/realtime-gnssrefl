use defmt::{info, Format};
use embassy_time::Instant;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use heapless::Vec;

use crate::messages::{MeasureReqMsg, MeasureResMsg};
use crate::nmea::NMEAParser;
use crate::realtime::Deviation;
use crate::storage::{BinStorage};
use crate::types::{Config, Sector, BIN_BURST_SIZE, BURST_SIZE};
use crate::utils::{seconds_to_time_str, date_from_days};
use crate::StorageType;
use crate::gnss::GNSSSensor;

#[derive(Debug, Format)]
pub enum SectorFailError {
    BinOverflow,
    StorageAccess,
    StorageWrite,
}

#[embassy_executor::task]
pub async fn task_measure(
    channel_req: &'static Channel<CriticalSectionRawMutex, MeasureReqMsg, 8>, 
    channel_res: &'static Channel<CriticalSectionRawMutex, MeasureResMsg, 8>, 
    storage: &'static StorageType,
    mut gnss_sensor: GNSSSensor
) {
    info!("[meas] starting");
    loop {
        info!("[meas] waiting for request...");
        let message = channel_req.receive().await;
        match message {
            MeasureReqMsg::GetRefTime => {
                info!("[meas] fetching reference time from GNSS");
                gnss_sensor.wake().await;
                if let Some((deviation, date)) = get_time(&mut gnss_sensor).await {
                    channel_res.send(MeasureResMsg::RefTimeSuccess { deviation, date }).await;
                } else {
                    channel_res.send(MeasureResMsg::RefTimeFail).await;
                }
                gnss_sensor.sleep().await;
            }
            MeasureReqMsg::MeasureSector { sector, config, sleep_gnss } => {
                info!("[meas] starting measurement for sector {}", sector.get_measurement_index());
                gnss_sensor.wake().await;
                let result = run_measure(&mut gnss_sensor, storage, &sector, &config).await;
                if sleep_gnss {
                    gnss_sensor.sleep().await;
                }
                if result.is_err() {
                    let error = result.err().unwrap();
                    info!("[meas] measurement for sector {} failed: {:?}", sector.get_measurement_index(), error);
                    channel_res.send(MeasureResMsg::SectorFail { error }).await;
                } else {
                    info!("[meas] measurement for sector {} completed successfully", sector.get_measurement_index());
                    channel_res.send(MeasureResMsg::SectorSuccess { 
                        sector_uid: sector.get_uid(), 
                        deviation: result.unwrap() 
                    }).await;
                }
            }
        }
    }
    
}

const BIN_DATA_SIZE: usize = 4 * BURST_SIZE * BIN_BURST_SIZE;

async fn run_measure(gnss_sensor: &mut GNSSSensor, storage: &'static StorageType, sector: &Sector, config: &Config) -> Result<Deviation, SectorFailError> {
    let bin_storage = BinStorage::new();
    let mut bin_data = Vec::<u8, BIN_DATA_SIZE>::new();
    let mut last_bin_id: u32 = sector.get_start_bin_index();
    let deviation: Deviation;
    let mut parser = NMEAParser::new_from_config(config);

    loop {
        let nmeaburst= gnss_sensor.read_burst().await;
        let burst = parser.parse_burst(&nmeaburst, false);
        let time_str = seconds_to_time_str(burst.time);

        if sector.is_time_before_sector(burst.time) {
            info!("[meas][{}] time is before current sector, skipping", time_str.as_str());
            continue;
        }

        if burst.time == u32::MAX {
            info!("[meas][{}] no valid GPS time in burst, skipping", time_str.as_str());
            continue;
        }

        if sector.is_time_after_sector(burst.time) {
            deviation = Deviation::new(burst.time, Instant::now() - nmeaburst.duration);
            info!("[meas][{}] time is later than current sector, STOPPING", time_str.as_str());
            break;
        }

        info!(
            "[meas][{}] received {} bytes in {} ms and parsed {} valid samples", 
            time_str.as_str(),
            nmeaburst.bytes,
            nmeaburst.duration.as_millis(),
            burst.samples.len()
        );

        let bin_id = sector.get_bin_for_time(burst.time);

        if bin_id != last_bin_id {
            write_bin(storage, &bin_storage, last_bin_id, &bin_data).await?;
            bin_data.clear();
            info!("[meas][{}] switched to new bin {}, wrote previous bin {} to storage", time_str.as_str(), bin_id, last_bin_id);
            last_bin_id = bin_id;
        }

        bin_data.extend_from_slice(&burst.to_bytes()).map_err(|_| SectorFailError::BinOverflow)?;

        info!("[meas][{}] burst added to current bin {} (size: {} bytes)", time_str.as_str(), bin_id, bin_data.len());
    }

    write_bin(storage, &bin_storage, last_bin_id, &bin_data).await?;
    bin_data.clear();
    info!("[meas] wrote last bin {} to storage", last_bin_id);

    Ok(deviation)
}

async fn write_bin(storage: &'static StorageType, bin_storage: &BinStorage, bin_id: u32, bin_data: &Vec<u8, BIN_DATA_SIZE>) -> Result<(), SectorFailError> {
    let mut storage_lock = storage.lock().await;
    let storage = storage_lock.as_mut().ok_or(SectorFailError::StorageAccess)?;
    bin_storage.write(storage, bin_id, bin_data).map_err(|_| SectorFailError::StorageWrite)
}

pub async fn get_time(gnss_sensor: &mut GNSSSensor) -> Option<(Deviation, u32)> {
    gnss_sensor.read_burst().await;
    let mut parser = NMEAParser::new();
    let nmeaburst = gnss_sensor.read_burst().await;
    let burst = parser.parse_burst(&nmeaburst, true);
    if let Some(date) = burst.date {
        if date > 40000 {
            let date = date_from_days(date as i64);
            info!("[meas] WARNING: parsed date seems wrong: {} is {}-{}-{}", date, date.2, date.1, date.0);
            None
        } else {
            let deviation: Deviation = Deviation::new(burst.time, Instant::now() - nmeaburst.duration);
            Some((deviation, date))
        }
    } else {
        None
    }
}