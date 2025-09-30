
use core::u32;

use defmt::info;
use embassy_time::{Instant, Timer};
use crate::types::{self, Config, Sector, NUM_MEASUREMENTS};
use crate::{utils, NUM_BINS};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use heapless::Vec;
use heapless::Deque;
use embassy_futures::select::{select4, Either4};

const SECONDS_PER_DAY: u32 = 86400;
const SECTOR_MARGIN: u32 = 60;
const SPEEDUP_FACTOR: u64 = 10;

#[embassy_executor::task]
pub async fn task_control(
    measure_request_channel: &'static Channel<CriticalSectionRawMutex, MeasureReqMsg, 8>,
    compute_request_channel: &'static Channel<CriticalSectionRawMutex, ComputeReqMsg, 8>,
    comm_request_channel: &'static Channel<CriticalSectionRawMutex, CommReqMsg, 8>,
    measure_response_channel: &'static Channel<CriticalSectionRawMutex, MeasureResMsg, 8>,
    compute_response_channel: &'static Channel<CriticalSectionRawMutex, ComputeResMsg, 8>,
    comm_response_channel: &'static Channel<CriticalSectionRawMutex, CommResMsg, 8>,
) {
    let config = types::Config::default();
    info!("[cont] starting");
    info!(
        "[cont] configuration: [{}] measurements ({})", 
        config.get_mid_times_as_str().as_str(),
        config.sector_mid_times.len()
    );
    let mut realtime = RealTime::new();

    // List of tasks
    let mut realtime_available = false;
    let mut sector: Deque<Sector, 1> = Deque::new();
    let mut list_measured: Deque<Sector, 8> = Deque::new();
    let mut list_computed: Deque<Sector, 8> = Deque::new();

    let mut next_sector: Deque<Sector, 1> = Deque::new();
    let mut next_timer_future = Timer::after_secs(u32::max_value() as u64);

    let mut core0busy = false;
    let mut core1busy = false;

    // TODO: Fill list_measured and list_computed from memory. 
    // This is for if there are still pending tasks from before power down
    
    loop {
        // Send task to Core 0 if available (get reference time or measure)
        if !core0busy {
            if !realtime_available {
                info!("[cont] requesting reference time from GNSS");
                measure_request_channel.send(MeasureReqMsg::GetRefTime).await;
                core0busy = true;
            } else
            if let Some(sector) = sector.front() {
                info!("[cont] requesting measurement for sector {}", sector.get_index());
                measure_request_channel.send(MeasureReqMsg::MeasureSector { sector: sector.clone(), config: config.clone() }).await;
                core0busy = true;
            }
        }

        // Send task to Core 1 if available (compute or communicate)
        if !core1busy {
            if let Some(sector) = list_measured.front() {
                info!("[cont] requesting computation for sector {}", sector.get_index());
                compute_request_channel.send(ComputeReqMsg::Compute { sector: sector.clone(), config: config.clone() }).await;
                core1busy = true;
            } else
            if let Some(sector) = list_computed.front() {
                info!("[cont] requesting communication for sector {}", sector.get_index());
                comm_request_channel.send(CommReqMsg::Send { sector: sector.clone(), config: config.clone()}).await;
                core1busy = true;
            }
        }

        // Wait for responses
        info!("[cont] waiting for events");
        let result = select4(
            &mut next_timer_future,
            measure_response_channel.receive(),
            compute_response_channel.receive(),
            comm_response_channel.receive(),
        ).await;
        match result {
            Either4::First(_) => {
                next_timer_future = Timer::after_secs(u32::max_value() as u64);
                info!("[cont] next sector timer expired, getting next sector");
                sector.push_back(next_sector.pop_front().unwrap()).unwrap();
                // 5 sec before next_sector
                let (ns, ntf) = next_sector_timer(&config, &realtime, SECTOR_MARGIN);
                next_sector.push_front(ns).unwrap();
                next_timer_future = ntf;
            }
            Either4::Second(measure_res_msg) => {
                match measure_res_msg {
                    MeasureResMsg::RefTimeSuccess { reftime, refdate} => {
                        info!("[cont] received reference time from GNSS");
                        realtime.update_ref(reftime, refdate);
                        realtime_available = true;
                        let (ns, ntf) = next_sector_timer(&config, &realtime, 0);
                        next_sector.push_front(ns).unwrap();
                        next_timer_future = ntf;
                        core0busy = false;
                    },
                    MeasureResMsg::RefTimeFail {} => {
                        info!("[cont] getting ref time failed");
                        core0busy = false;
                    }
                    MeasureResMsg::SectorSuccess { } => {
                        info!("[cont] received sector measurement success from GNSS");
                        let sector = sector.pop_front().unwrap();
                        list_measured.push_back(sector).unwrap();
                        core0busy = false;
                    },
                    _ => {}
                }
            }
            Either4::Third(compute_res_msg) => {
                match compute_res_msg {
                    ComputeResMsg::Success => {
                        info!("[cont] received computation success from core 1");
                        let sector = list_measured.pop_front().unwrap();
                        list_computed.push_back(sector).unwrap();
                        core1busy = false;
                    }
                    _ => {}
                }
            }
            Either4::Fourth(comm_res_msg) => {
                match comm_res_msg {
                    CommResMsg::Success => {
                        info!("[cont] received communication success from core 1");
                        list_computed.pop_front().unwrap();
                        core1busy = false;
                    }
                    _ => {}
                }
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
}

pub enum MeasureResMsg {
    RefTimeSuccess {
        reftime: u32,
        refdate: u32,
    },
    RefTimeFail,
    SectorSuccess,
    SectorFail, 
}

pub enum ComputeReqMsg {
    Compute {
        sector: Sector,
        config: Config,
    }
}

pub enum ComputeResMsg {
    Success,
    Fail 
}

pub enum CommReqMsg{
    Send {
        sector: Sector,
        config: Config,
    }
}

pub enum CommResMsg{
    Success,
    Fail
}

struct RealTime {
    ref_real_date: u32,
    ref_real_time: u32,
    ref_pico_time: Instant
}

impl RealTime {
    pub fn new() -> Self{
        Self {
            ref_real_date: 0,
            ref_real_time: 0,
            ref_pico_time: Instant::from_secs(Instant::now().as_secs()*SPEEDUP_FACTOR)
        }
    }

    fn get_real_time(&self) -> u32 {
        let time = self.ref_real_time + Instant::from_secs(Instant::now().as_secs()*SPEEDUP_FACTOR)
        .duration_since(self.ref_pico_time).as_secs() as u32;
        (time % 86400) as u32
    }

    fn get_days(&self) -> u32 {
        return self.ref_real_date;
    }

    fn update_ref(&mut self, ref_time: u32, ref_date: u32) {
        self.ref_real_time = ref_time;
        self.ref_real_date = ref_date;
        self.ref_pico_time = Instant::from_secs(Instant::now().as_secs()*SPEEDUP_FACTOR);
        info!(
            "[cont] updated reference time to {} ({}) and date to {} ({}) (pico at {} ms since startup)", 
            ref_time, 
            utils::seconds_to_time_str(ref_time).as_str(), 
            ref_date, utils::date_to_str(ref_date).as_str(), 
            self.ref_pico_time.as_millis()
        );
    }

    fn next_or_min(vec: &Vec<u32, 24>, x: u32) -> (u32, usize) {
        vec.iter()
            .enumerate()
            .find(|&(_, &v)| v > x)
            .map(|(i, &v)| (v, i))
            .unwrap_or((vec[0], 0))
    }

    // Assumes one measurement per day
    fn subtract_wrapping(t1: u32, t2: u32) -> u32 {
        (t1 - t2 + SECONDS_PER_DAY) % SECONDS_PER_DAY
    }

    fn add_wrapping(t1: u32, t2: u32) -> u32 {
        (t1 + t2) % SECONDS_PER_DAY
    }
}

fn next_sector_timer<'a>(
    config: &Config,
    realtime: &RealTime,
    offset: u32,
) -> (Sector, Timer) {
    let sectors_per_day = config.sector_mid_times.len() as u32;
    let half_measure_duration = config.get_measure_duration() / 2;
    let sector_start_times: Vec<u32, 24> = config
        .sector_mid_times
        .iter()
        .map(|&midpoint| RealTime::subtract_wrapping(midpoint , half_measure_duration))
        .collect();
    let expected_time = RealTime::add_wrapping(realtime.get_real_time(), offset);
    let (start_time, nth_of_day) = RealTime::next_or_min(&sector_start_times, expected_time);

    let is_next_day = start_time <= expected_time;
    let sector_day = if is_next_day { realtime.get_days() + 1 } else { realtime.get_days() };

    // Compute sector index
    let sector_index = (sector_day * sectors_per_day
        + nth_of_day as u32) % NUM_MEASUREMENTS as u32;

    // Compute bin range
    let start_bin_index = (sector_day * sectors_per_day as u32
        + nth_of_day as u32 * config.bins_per_sector)
        % NUM_BINS as u32;
    let n_bins = config.bins_per_sector;

    // Create Sector
    let sector = Sector::new(
        sector_index as u32,
        start_bin_index,
        start_time,
        n_bins,
        config.seconds_per_bin,
    );

    info!(
        "[cont] next sector {} starts at {} s ({}) on day {} ({}) with bins {:?}", 
        sector.get_index(), 
        sector.get_start_time(), 
        utils::seconds_to_time_str(sector.get_start_time()).as_str(),
        sector_day,
        utils::date_to_str(sector_day).as_str(),
        sector.get_bins().as_slice()
    );

    // Sleep until start of next sector (could be the next day)
    let start_time_margin = RealTime::subtract_wrapping(start_time, SECTOR_MARGIN);
    let sleeptime = RealTime::subtract_wrapping(start_time_margin, realtime.get_real_time());

    info!(
        "[cont] sleeping from {} ({}) for {} s ({}) until next sector starts", 
        realtime.get_real_time(),
        utils::seconds_to_time_str(realtime.get_real_time()).as_str(),
        sleeptime, 
        utils::seconds_to_time_str(sleeptime).as_str()
    );

    (sector, Timer::after_millis(sleeptime as u64 * 1000 / SPEEDUP_FACTOR))
}