
use core::u32;

use defmt::info;
use embassy_time::Timer;
use crate::realtime::{Deviation, RealTime};
use crate::types::{self, Config, Sector};
use crate::scheduler::*;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
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
) {
    let config = types::Config::default();
    info!("[cont] starting");
    info!(
        "[cont] configuration: [{}] measurements ({})", 
        config.get_mid_times_as_str().as_str(),
        config.sector_mid_times.len()
    );
    let mut realtime = RealTime::new();
    let scheduler = Scheduler::new();

    // List of tasks
    let mut realtime_available = false;
    let mut list_measured: Deque<Sector, 8> = Deque::new();
    let mut list_computed: Deque<Sector, 8> = Deque::new();

    let mut sector: Deque<Sector, 1> = Deque::new();
    let mut sectortimer: Option<SectorTime> = None;

    // TODO: Fill list_measured and list_computed from memory. 
    // This is for if there are still pending tasks from before power down
    
    loop {
        // Send out tasks
        if !realtime_available {
            info!("[cont] requesting reference time from GNSS");
            measure_request_channel.send(MeasureReqMsg::GetRefTime).await;
        } else if let Some(sector) = sector.pop_front() {
            info!("[cont] requesting measurement for sector {}", sector.get_measurement_index());

            let sleep_gnss = if let Some(sectortimer) = sectortimer.as_ref() {
                sectortimer.sector.is_directly_following(&sector)
            } else {
                false
            };

            measure_request_channel.send(MeasureReqMsg::MeasureSector { sector: sector.clone(), config: config.clone(), sleep_gnss }).await;
        }

        if let Some(sector) = list_measured.front() {
            info!("[cont] requesting computation for sector {}", sector.get_measurement_index());
            compute_request_channel.send(ComputeReqMsg::Compute { sector: sector.clone(), config: config.clone() }).await;
        }
        if let Some(sector) = list_computed.front() {
            info!("[cont] requesting communication for sector {}", sector.get_measurement_index());
            comm_request_channel.send(CommReqMsg::Send { sector: sector.clone(), config: config.clone()}).await;
        }

        // Wait for responses
        info!("[cont] waiting for events");

        let mut timer = if let Some(st) = sectortimer.as_ref() {
            realtime.get_timer(st.sleep_until)
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
                let current_sectortime = sectortimer.expect("There should be a sector here");
                sectortimer = Some(scheduler.get_next_sectortimer(&config, &realtime, &current_sectortime.sector));
                sector.push_back(current_sectortime.sector).expect("Should fit");
            }
            Either4::Second(measure_res_msg) => {
                match measure_res_msg {
                    MeasureResMsg::RefTimeSuccess { deviation, date} => {
                        info!("[cont] received reference time from GNSS");
                        realtime.update_time(deviation);
                        realtime.update_date(date);
                        realtime_available = true;
                        sectortimer = Some(scheduler.get_first_sectortimer(&config, &realtime));
                    },
                    MeasureResMsg::RefTimeFail {} => {
                        info!("[cont] getting ref time failed");
                    }
                    MeasureResMsg::SectorSuccess { deviation } => {
                        info!("[cont] received sector measurement success from GNSS");
                        let sector = sector.pop_front().unwrap();
                        list_measured.push_back(sector).unwrap();
                        realtime.update_time(deviation);
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
                    }
                    _ => {}
                }
            }
            Either4::Fourth(comm_res_msg) => {
                match comm_res_msg {
                    CommResMsg::Success => {
                        info!("[cont] received communication success from core 1");
                        list_computed.pop_front().unwrap();
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


