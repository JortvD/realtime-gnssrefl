
use core::u32;

use embassy_rp::config;
use embassy_time::Duration;

use embassy_rp::uart::{Uart, Async};
use embassy_rp::gpio::Output;
use embassy_executor::Spawner;
use embassy_time::{Instant, Timer};

use crate::gnss::GNSSSensor;
use crate::rockblock::RockBlock;
use crate::storage::{BinStorage, FlashStorage};
use crate::types::{self, Config, Sector};
use crate::nmea::NMEAParser;
use crate::NUM_BINS;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use heapless::Vec;
use heapless::Deque;
use embassy_futures::select::{select4, Either4};


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
    let mut list_measured: Deque<Sector, 8> = Deque::new();
    let mut list_computed: Deque<u32, 8> = Deque::new();
    let mut next_timer_future = Timer::after_secs(u32::max_value() as u64);
    let mut next_sector = Sector::new(0,0,0,0,0 );

    // Start the control getting a reference time
    measure_request_channel.send(MeasureReqMsg::GetRefTime).await;

    // TODO: Fill list_measured and list_computed from memory. 
    // This is for if there are still pending tasks from before power down
    
    loop {
        //TODO: Send out tasks if possible

        //Wait for responses
        let result = select4(
        &mut next_timer_future,
        measure_response_channel.receive(),
        compute_response_channel.receive(),
        comm_response_channel.receive(),
        ).await;
        match result {
            Either4::First(_) => {
                measure_request_channel.send(MeasureReqMsg::MeasureSector { sector: next_sector, config: config.clone()}).await;
                next_sector_timer(&config, &realtime); 
            }
            Either4::Second(measure_res_msg) => {
                match measure_res_msg {
                    MeasureResMsg::GiveRefTime { reftime} => {
                        realtime.update_ref_time(reftime);
                        (next_sector, next_timer_future) = next_sector_timer(&config, &realtime);
                    },
                    MeasureResMsg::SectorSuccesful { sector } => {
                        list_measured.push_back(sector);
                    },
                    _ => {}
                }
            }
            Either4::Third(compute_res_msg) => {
                // TODO: Push compute sector index to list
                // Remove sector from measured
            }
            Either4::Fourth(comm_res_msg) => {
                // TODO: Remove sectorindex from compute list 
            }
        }
            
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
    },
    SectorSuccesful{
        sector: Sector,
    },
    SectorFail, 
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

    fn get_days(&self) -> u32 {
        //TODO: get #days since arbitrary day in history
        return 0;
    }

    fn update_ref_time(&mut self, ref_time: u32) {
        self.ref_real_time = ref_time;
        self.ref_pico_time = Instant::now();
    }

    fn next_or_min(vec: &Vec<u32, 24>, x: u32) -> (u32, usize) {
        vec.iter()
            .enumerate()
            .find(|&(_, &v)| v > x)
            .map(|(i, &v)| (v, i))
            .unwrap_or((vec[0], 0))
    }


    fn subtract_wrapping(t1: u32, t2: u32) -> u32{
        (t1 - t2) % 86400
    }
}


fn next_sector_timer<'a>(
    config: &Config,
    realtime: &RealTime,
) -> (Sector, Timer) {
    let sectors_per_day = config.sector_midpoints.len();

    //TODO: find next startpoint instead of midpoint
    // Find the next midpoint and its index
    let (midpoint, sector_index) =
        RealTime::next_or_min(&config.sector_midpoints, realtime.get_real_time());

    // Compute sector start time
    let start_time = RealTime::subtract_wrapping(midpoint, config.sector_measure_duration / 2);

    // Compute bin range
    let start_bin_id = (realtime.get_days() * sectors_per_day as u32
        + sector_index as u32 * config.bins_per_sector)
        % NUM_BINS as u32;
    let n_bins = config.bins_per_sector;

    // Create Sector
    let sector = Sector::new(
        sector_index as u32,
        start_bin_id,
        start_time,
        n_bins,
        config.seconds_per_bin,
    );

    // Compute sleep duration
    let sleeptime = start_time - realtime.get_real_time();

    // Create timer future
    let timer_future = Timer::after_secs(sleeptime as u64);

    (sector, timer_future)
}

// // Assume sector_midpoints is ordered.
// let sectors_per_day = config.sector_midpoints.len();
// let (midpoint, sector_index) = RealTime::next_or_min(&config.sector_midpoints, realtime.get_real_time());
// let start_time = RealTime::subtract_wrapping(midpoint, config.sector_measure_duration / 2);

// let start_bin_id = (realtime.get_days()*sectors_per_day as u32 + sector_index as u32 * config.bins_per_sector) % NUM_BINS as u32; //TODO
// let n_bins = config.bins_per_sector;
// let sector = Sector::new(sector_index as u32, start_bin_id, start_time, n_bins, config.seconds_per_bin);

// let sleeptime = start_time - realtime.get_real_time();
// let mut timer_future = Timer::after_secs(sleeptime as u64);