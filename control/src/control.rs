
use core::u32;

use defmt::{error, info};
use embassy_time::Timer;
use crate::realtime::{Deviation, RealTime};
use crate::types::{self, Config, Sector};
use crate::scheduler::*;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use heapless::{Deque, Vec};
use embassy_futures::select::{select4, Either4};

pub const MAX_SECTORS: usize = 128;

struct SectorList {
    sectors: Vec<Sector, MAX_SECTORS>,
}

impl SectorList {
    pub fn new() -> Self {
        Self {
            sectors: Vec::new(),
        }
    }

    pub fn push(&mut self, sector: Sector) {
        self.sectors.push(sector).expect("Should fit");
    }

    pub fn delete_uid(&mut self, uid: u32) {
        let index = self.sectors.iter().position(|s| s.get_uid() == uid);
        if let Some(i) = index {
            self.sectors.remove(i);
        } else {
            error!("Tried to delete sector with uid {} but it was not found", uid);
        }
    }

    pub fn get_mut(&mut self, index: usize) -> &mut Sector {
        self.sectors.get_mut(index).expect("Only use after getting id directly")
    }

    pub fn get(&self, index: usize) -> &Sector {
        self.sectors.get(index).expect("Only use after getting id directly")
    }

    pub fn get_uid(&self, uid: u32) -> Option<&Sector> {
        for sector in self.sectors.iter() {
            if sector.get_uid() == uid {
                return Some(sector);
            }
        }
        None
    }

    pub fn get_mut_uid(&mut self, uid: u32) -> Option<&mut Sector> {
        for sector in self.sectors.iter_mut() {
            if sector.get_uid() == uid {
                return Some(sector);
            }
        }
        None
    }

    pub fn get_idx_for_state(&self, state: types::SectorState) -> Option<usize> {
        for (i, sector) in self.sectors.iter().enumerate() {
            if sector.state == state {
                return Some(i);
            }
        }
        None
    }

    pub fn get_idxs_for_state(&self, state: types::SectorState) -> Vec<usize, MAX_SECTORS> {
        let mut indices = Vec::<usize, MAX_SECTORS>::new();
        for (i, sector) in self.sectors.iter().enumerate() {
            if sector.state == state {
                indices.push(i).expect("Should fit");
            }
        }
        indices
    }
}

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
    let mut sectors = SectorList::new();
    let mut sleep_until: Option<u32> = None;

    // TODO: Fill list_measured and list_computed from memory. 
    // This is for if there are still pending tasks from before power down
    
    loop {
        // Send out tasks
        if !realtime_available {
            info!("[cont] requesting reference time from GNSS");
            measure_request_channel.send(MeasureReqMsg::GetRefTime).await;
        } else if let Some(index) = sectors.get_idx_for_state(types::SectorState::TO_MEASURE) {
            let measuring_idxs: Vec<usize, MAX_SECTORS> = sectors.get_idxs_for_state(types::SectorState::MEASURING);

            let sector = sectors.get(index);
            info!("[cont] requesting measurement for sector {}", sector.get_measurement_index());

            let preceding_sector = measuring_idxs.iter().find(|&i| {
                sectors.get(*i).is_succeeding(&sector)
            });
            let sleep_gnss = preceding_sector.is_none();

            let sector = sectors.get_mut(index);
            sector.state = types::SectorState::MEASURING;
            measure_request_channel.send(MeasureReqMsg::MeasureSector { sector: sector.clone(), config: config.clone(), sleep_gnss }).await;
        }

        let list_to_compute = sectors.get_idxs_for_state(types::SectorState::TO_COMPUTE);
        if list_to_compute.len() > 0 {
            for &index in list_to_compute.iter() {
                let sector = sectors.get_mut(index);
                info!("[cont] requesting computation for sector {}", sector.get_measurement_index());
                sector.state = types::SectorState::COMPUTING;
                compute_request_channel.send(ComputeReqMsg::Compute { sector: sector.clone(), config: config.clone() }).await;
            }
        }

        let list_to_communicate = sectors.get_idxs_for_state(types::SectorState::TO_COMMUNICATE);
        if list_to_communicate.len() > 0 {
            let mut sectors_to_send = Vec::<Sector, MAX_SECTORS>::new();
            for &index in list_to_communicate.iter() {
                let sector = sectors.get_mut(index);
                info!("[cont] scheduling communication for sector {}", sector.get_measurement_index());
                sector.state = types::SectorState::COMMUNICATING;
                sectors_to_send.push(sector.clone()).expect("Should fit");
            }
            info!("[cont] requesting communication for {} sectors", sectors_to_send.len());
            comm_request_channel.send(CommReqMsg::Send { sectors: sectors_to_send, config: config.clone() }).await;
        }

        // Wait for responses
        info!("[cont] waiting for events");

        let mut timer = if let Some(st) = sleep_until {
            realtime.get_timer(st)
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
                let sector_awaiting_idx = sectors.get_idx_for_state(types::SectorState::AWAITING);
                if let Some(idx) = sector_awaiting_idx {
                    let sector = sectors.get_mut(idx);
                    sector.state = types::SectorState::TO_MEASURE;

                    let (next_sector, next_start_time) = scheduler.get_next_sector(&config, &realtime, sector);
                    sleep_until = Some(next_start_time);
                    sectors.push(next_sector);
                } else {
                    panic!("No sector in AWAITING state when timer expired");
                }
            }
            Either4::Second(measure_res_msg) => {
                match measure_res_msg {
                    MeasureResMsg::RefTimeSuccess { deviation, date} => {
                        info!("[cont] received reference time from GNSS");
                        realtime.update_time(deviation);
                        realtime.update_date(date);
                        realtime_available = true;
                        let (first_sector, first_start_time) = scheduler.get_first_sector(&config, &realtime);
                        sleep_until = Some(first_start_time);
                        sectors.push(first_sector);
                    },
                    MeasureResMsg::RefTimeFail {} => {
                        info!("[cont] getting ref time failed");
                    }
                    MeasureResMsg::SectorSuccess { sector_uid, deviation } => {
                        info!("[cont] received sector measurement success from GNSS");
                        let sector = sectors.get_mut_uid(sector_uid).expect("Sector should exist");
                        sector.state = types::SectorState::TO_COMPUTE;
                        realtime.update_time(deviation);
                    },
                    _ => {}
                }
            }
            Either4::Third(compute_res_msg) => {
                match compute_res_msg {
                    ComputeResMsg::Success { sector_uid } => {
                        info!("[cont] received computation success from core 1");
                        let sector = sectors.get_mut_uid(sector_uid).expect("Sector should exist");
                        sector.state = types::SectorState::TO_COMMUNICATE;
                    }
                    _ => {}
                }
            }
            Either4::Fourth(comm_res_msg) => {
                match comm_res_msg {
                    CommResMsg::Success { sector_uids } => {
                        info!("[cont] received communication success from core 1");
                        for &sector_uid in sector_uids.iter() {
                            sectors.delete_uid(sector_uid);
                        }
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
        sector_uid: u32,
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
    Success {
        sector_uid: u32,
    },
    Fail 
}

pub enum CommReqMsg{
    Send {
        sectors: Vec<Sector, MAX_SECTORS>,
        config: Config,
    }
}

pub enum CommResMsg{
    Success {
        sector_uids: Vec<u32, MAX_SECTORS>,
    },
    Fail
}


