use defmt::info;
use embassy_time::Instant;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use heapless::Vec;

use crate::control::{MeasureReqMsg, MeasureResMsg};
use crate::StorageType;
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
        match channel_req.receive().await {
            MeasureReqMsg::GetRefTime => {
                {
                    let reftime = get_time(&mut gnss_sensor).await;
                    channel_res.send(MeasureResMsg::GiveRefTime { reftime: reftime }).await;
                }
            }
            MeasureReqMsg::MeasureSector { sector, config } => {
                {
                    run_measure(&mut gnss_sensor, storage, &sector, &config).await;
                    channel_res.send(MeasureResMsg::SectorSuccesful { sector }).await;
                }
            }
            
        }
    }
    
}

async fn run_measure(gnss_sensor: &mut GNSSSensor, storage: &'static StorageType, sector: &Sector, config: &Config) {
    let bin_storage = BinStorage::new(config.seconds_per_bin);
    let mut bin_data = Vec::<u8, {4 * BURST_SIZE * BIN_BURST_SIZE}>::new();
    let mut last_bin_id: u32 = sector.get_start_bin_id();

    loop {
        let nmeaburst= gnss_sensor.read_burst().await;
        info!("[measure][????????] received NMEA burst of {} bytes in {} ms", nmeaburst.bytes, nmeaburst.duration.as_millis());
        let start_time = Instant::now();

        // A full burst has been collected, do something with it
        let burst = gnss_sensor.parser.parse_burst(&nmeaburst);

        let time_str = seconds_to_time_str(burst.time);

        if sector.is_time_before_sector(burst.time) {
            info!("[measure][{}] time is before current sector, skipping", time_str.as_str());
            continue;
        }

        if burst.time == u32::MAX {
            info!("[measure][{}] no valid GPS time in burst, skipping", time_str.as_str());
            continue;
        }

        if sector.is_time_after_sector(burst.time) {
            info!("[measure][{}] time is later than current sector, STOPPING", time_str.as_str());
            break;
        }

        info!("[measure][{}] parsed burst with {} samples", time_str.as_str(), burst.num);

        let bin_id = sector.get_bin_for_time(burst.time);

        if bin_id != last_bin_id {
            {
                let mut storage_lock = storage.lock().await;
                let storage = storage_lock.as_mut().expect("Storage not initialized");
                bin_storage.write(storage, last_bin_id, &bin_data).expect("Failed to write bin to storage");
            }
            bin_data.clear();
            info!("[measure][{}] switched to new bin {}, wrote previous bin {} to storage", time_str.as_str(), bin_id, last_bin_id);
            last_bin_id = bin_id;
        }

        let data = burst.to_bytes();

        bin_data.extend_from_slice(&data).expect("Bin data overflow");

        let elapsed = Instant::now() - start_time;
        info!("[measure][{}] burst added to cached bin {}, elapsed {} ms, cache size {} bytes", time_str.as_str(), bin_id, elapsed.as_millis(), bin_data.len());
    }

    {
        let mut storage_lock = storage.lock().await;
        let storage = storage_lock.as_mut().expect("Storage not initialized");
        bin_storage.write(storage, last_bin_id, &bin_data).expect("Failed to write bin to storage");
    }
    bin_data.clear();
    info!("[measure][????????] wrote last bin {} to storage", last_bin_id);
}

pub async fn get_time(gnss_sensor: &mut GNSSSensor) -> u32 {
    let nmeaburst = gnss_sensor.read_burst().await;
    let burst = gnss_sensor.parser.parse_burst(&nmeaburst);
    burst.time
}