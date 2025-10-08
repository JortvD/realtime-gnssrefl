use heapless::Vec;

use crate::{compute::ComputeError, measure::SectorFailError, realtime::Deviation, types::{Config, Sector, MAX_SECTORS}};

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
    SectorFail {
        error: SectorFailError,
    }
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
    ComputeFail {
        error: ComputeError
    }
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
