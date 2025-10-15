use heapless::Vec;

use crate::{comms::CommsError, compute::ComputeError, measure::{MeasureResult, SectorFailError}, realtime::Deviation, types::{Config, Sector, MAX_SECTORS}};

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
        result: MeasureResult,
    },
    SectorFail {
        sector_uid: u32,
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
        sector_uid: u32,
        error: ComputeError
    }
}

pub enum CommReqMsg{
    Send {
        sectors: Vec<Sector, MAX_SECTORS>,
        config: Config,
        battery_mv: Option<u32>,
        temp_c: Option<f32>
    },
}

pub enum CommResMsg{
    Success {
        sector_uids: Vec<u32, MAX_SECTORS>,
    },
    Fail {
        sector_uids: Vec<u32, MAX_SECTORS>,
        error: CommsError
    }
}

pub enum MonReqMsg{
    GetBatVolt,
    GetTemp
}

pub enum MonResMsg{
    BatVoltSuccess{
        voltage: u32
    },
    BatVoltFail,
    TempSuccess{
        temp_c: f32
    },
    TempFail
}
