
use embassy_time::Duration;

use embassy_rp::uart::{Uart, Async};
use embassy_rp::gpio::Output;
use embassy_executor::Spawner;
use embassy_time::{Instant, Timer};

use crate::gnss::GNSSSensor;
use crate::rockblock::RockBlock;
use crate::storage::{BinStorage, FlashStorage};
use crate::StorageType;
use crate::types::{self, Config, Sector};
use crate::nmea::NMEAParser;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use heapless::Vec;

#[embassy_executor::task]
pub async fn task_control(
    measure_request_channel: &'static Channel<CriticalSectionRawMutex, MeasureReqMsg, 8>,
    compute_request_channel: &'static Channel<CriticalSectionRawMutex, ComputeReqMsg, 8>,
    comm_request_channel: &'static Channel<CriticalSectionRawMutex, CommReqMsg, 8>,
    measure_response_channel: &'static Channel<CriticalSectionRawMutex, MeasureResMsg, 8>,
    compute_response_channel: &'static Channel<CriticalSectionRawMutex, ComputeResMsg, 8>,
    comm_response_channel: &'static Channel<CriticalSectionRawMutex, CommResMsg, 8>,
    ) 
    {

    let config = types::Config::default();
    let mut realtime = RealTime::new();

    measure_request_channel.send(MeasureReqMsg::GetRefTime).await;
    match measure_response_channel.receive().await {
        MeasureResMsg::GiveRefTime { reftime} => {
            realtime.update_ref_time(reftime);
        }
        _ => {
            
        }
    }

    loop {
        let next_midpoint = next_or_min(&config.sector_midpoints, realtime.get_real_time());
        let next_startpoint = next_midpoint - &config.sector_measure_duration / 2;
        
        let start_bin_id = 0; //TODO
        let n_bins = config.sector_measure_duration / config.bins_per_sector;
        let sector = Sector::new(0, start_bin_id, next_startpoint, n_bins, config.seconds_per_bin);

        let sleeptime = next_startpoint - realtime.get_real_time();
        let timer_future = Timer::after_secs(sleeptime as u64);
        //Use select to also wait for channel
        timer_future.await;

        // TODO: Make control loop that listens to the right channels at the right time
    }
}

pub enum MeasureReqMsg {
    GetRefTime,
    MeasureSector {
        sector: Sector,
        config: Config,
    }
    //TODO: UpdateConfig
}

pub enum MeasureResMsg {
    GiveRefTime {
        reftime: u32
    }
}

pub enum ComputeReqMsg {
    // TODO: Add requests
    letsgo {}
}

pub enum ComputeResMsg {
    // TODO: Add responses
    letsgo {}
}

pub enum CommReqMsg{
    // TODO: Add requests
    letsgo {}
}

pub enum CommResMsg{
    // TODO: Add responses
    letsgo {}
}

struct RealTime {
    ref_real_time: u32,
    ref_pico_time: Instant
}

impl RealTime {
    pub fn new() -> Self{
        Self {
           ref_real_time: 0,
           ref_pico_time: Instant::now(),
        }
    }

    fn get_real_time(&self) -> u32 {
        let time = self.ref_real_time + Instant::now().duration_since(self.ref_pico_time).as_secs() as u32;
        (time % 86400) as u32
    }

    // fn convert_to_pico_time(self, real_time: u32) -> Instant {
    //     real_time - self.ref_real_time 

    // }

    fn update_ref_time(&mut self, ref_time: u32) {
        self.ref_real_time = ref_time;
        self.ref_pico_time = Instant::now();
    }
}

fn next_or_min(vec: &Vec<u32, 24>, x: u32) -> u32 {
    // Try to find the smallest value bigger than x
    vec.iter()
        .filter(|&&v| v > x)
        .min()
        // fallback: if none found, return overall smallest
        .copied()
        .or_else(|| vec.iter().min().copied()).unwrap()
}