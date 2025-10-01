
use core::u32;

use defmt::info;
use embassy_time::{Instant, Timer};
use crate::realtime::{Deviation, RealTime};
use crate::types::{self, Config, Sector, NUM_MEASUREMENTS};
use crate::{utils, NUM_BINS};
use crate::scheduler::*;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use heapless::Vec;
use heapless::Deque;
use embassy_futures::select::{select4, Either4};

const SECONDS_PER_DAY: u32 = 86400;
const SECTOR_MARGIN: u32 = 60;

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
    let mut scheduler = Scheduler::new(&realtime);

    // List of tasks
    let mut realtime_available = false;
    // let mut sector: Deque<Sector, 1> = Deque::new();
    let mut list_measured: Deque<Sector, 8> = Deque::new();
    let mut list_computed: Deque<Sector, 8> = Deque::new();

    let mut sector: Deque<Sector, 1> = Deque::new();
    let mut sectortimer: Option<SectorTimer>;

    // let mut next_sector: Deque<Sector, 1> = Deque::new();
    // let mut next_timer_future = Timer::after_secs(u32::max_value() as u64);

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
            if let Some(sector) = sector.pop_front() {
                info!("[cont] requesting measurement for sector {}", sector.get_measurement_index());

                let sleep_gnss = if let Some(sectortimer) = sectortimer {
                    sectortimer.sector.is_directly_following(&sector)
                } else {
                    false
                };

                measure_request_channel.send(MeasureReqMsg::MeasureSector { sector: sector.clone(), config: config.clone(), sleep_gnss }).await;
                core0busy = true;
            }
        }

        // Send task to Core 1 if available (compute or communicate)
        if !core1busy {
            if let Some(sector) = list_measured.front() {
                info!("[cont] requesting computation for sector {}", sector.get_measurement_index());
                compute_request_channel.send(ComputeReqMsg::Compute { sector: sector.clone(), config: config.clone() }).await;
                core1busy = true;
            } else
            if let Some(sector) = list_computed.front() {
                info!("[cont] requesting communication for sector {}", sector.get_measurement_index());
                comm_request_channel.send(CommReqMsg::Send { sector: sector.clone(), config: config.clone()}).await;
                core1busy = true;
            }
        }

        // Wait for responses
        info!("[cont] waiting for events");

        let mut timer = if let Some(st) = sectortimer {
            RealTime::get_timer(st.timer)
        } else {
            Timer::after_secs(u64::MAX)
        };

        let result = select4(
            &mut timer,
            measure_response_channel.receive(),
            compute_response_channel.receive(),
            comm_response_channel.receive(),
        ).await;
        match result {
            Either4::First(_) => {
                info!("[cont] next sector timer expired, getting next sector");
                let finished_sector = sector.pop_front().unwrap();
                sectortimer = Some(scheduler.get_next_sectortimer(&config, &finished_sector));
                sector.push_back(finished_sector).unwrap();
            }
            Either4::Second(measure_res_msg) => {
                match measure_res_msg {
                    MeasureResMsg::RefTimeSuccess { deviation, date} => {
                        info!("[cont] received reference time from GNSS");
                        realtime.update(deviation, date);
                        realtime_available = true;
                        sectortimer = Some(scheduler.get_first_sectortimer(&config));
                        core0busy = false;
                    },
                    MeasureResMsg::RefTimeFail {} => {
                        info!("[cont] getting ref time failed");
                        core0busy = false;
                    }
                    MeasureResMsg::SectorSuccess { deviation } => {
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
        sleep_gnss: bool,
    }
}

pub enum MeasureResMsg {
    RefTimeSuccess {
        deviation: Deviation,
        date: u32,
    },
    RefTimeFail,
    SectorSuccess {
        deviation: Deviation,
    },
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


