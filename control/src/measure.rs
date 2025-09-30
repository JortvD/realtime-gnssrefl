use defmt::info;
use embassy_time::Instant;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use heapless::Vec;

use crate::control::{MeasureReqMsg, MeasureResMsg};
use crate::{utils, StorageType};
use crate::{gnss::GNSSSensor, storage::{BinStorage, FlashStorage}, types::{Config, Sector, BIN_BURST_SIZE, BURST_SIZE}, utils::seconds_to_time_str};

#[embassy_executor::task]
pub async fn task_measure(
    channel_req: &'static Channel<CriticalSectionRawMutex, MeasureReqMsg, 8>, 
    channel_res: &'static Channel<CriticalSectionRawMutex, MeasureResMsg, 8>, 
    storage: &'static StorageType,
    mut gnss_sensor: GNSSSensor
) {
    // Get sector & config
    loop {
        info!("[meas] Wait for request");
        let message = channel_req.receive().await;
        match message {
            MeasureReqMsg::GetRefTime => {
                info!("[meas] fetching reference time from GNSS");
                if let Some((reftime, refdate)) = get_time(&mut gnss_sensor).await {
                    channel_res.send(MeasureResMsg::RefTimeSuccess{ reftime, refdate }).await;
                } else {
                    channel_res.send(MeasureResMsg::RefTimeFail).await;
                }
            }
            MeasureReqMsg::MeasureSector { sector, config } => {
                info!("[meas] starting measurement for sector {}", sector.get_index());
                run_measure(&mut gnss_sensor, storage, &sector, &config).await;
                channel_res.send(MeasureResMsg::SectorSuccess).await;
            }
        }
    }
    
}

async fn run_measure(gnss_sensor: &mut GNSSSensor, storage: &'static StorageType, sector: &Sector, config: &Config) {
    let bin_storage = BinStorage::new(config.seconds_per_bin);
    let mut bin_data = Vec::<u8, {4 * BURST_SIZE * BIN_BURST_SIZE}>::new();
    let mut last_bin_id: u32 = sector.get_start_bin_index();

    loop {
        let nmeaburst= gnss_sensor.read_burst().await;
        let burst = gnss_sensor.parser.parse_burst(&nmeaburst, false);
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
            {
                let mut storage_lock = storage.lock().await;
                let storage = storage_lock.as_mut().expect("Storage not initialized");
                bin_storage.write(storage, last_bin_id, &bin_data).expect("Failed to write bin to storage");
            }
            bin_data.clear();
            info!("[meas][{}] switched to new bin {}, wrote previous bin {} to storage", time_str.as_str(), bin_id, last_bin_id);
            last_bin_id = bin_id;
        }

        let data = burst.to_bytes();

        bin_data.extend_from_slice(&data).expect("Bin data overflow");

        info!("[meas][{}] burst added to current bin {} (size: {} bytes)", time_str.as_str(), bin_id, bin_data.len());
    }

    {
        let mut storage_lock = storage.lock().await;
        let storage = storage_lock.as_mut().expect("Storage not initialized");
        bin_storage.write(storage, last_bin_id, &bin_data).expect("Failed to write bin to storage");
    }
    bin_data.clear();
    info!("[meas] wrote last bin {} to storage", last_bin_id);
}

pub async fn get_time(gnss_sensor: &mut GNSSSensor) -> Option<(u32, u32)> {
    gnss_sensor.read_burst().await;
    let nmeaburst = gnss_sensor.read_burst().await;
    let burst = gnss_sensor.parser.parse_burst(&nmeaburst, true);
    if let Some(date) = burst.date {
        if date > 40000 {
            let date = utils::date_from_days(date as i64);
            info!("[meas] WARNING: parsed date seems wrong: {} is {}-{}-{}", date, date.2, date.1, date.0);
            None
        } else {
            Some((burst.time, date))
        }
    } else {
        None
    }
}