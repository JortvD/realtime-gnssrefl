#[derive(Debug, Clone)]
pub struct Arc {
    pub sat_id: u32,
    pub time_start: i64,
    pub time_end: i64,
    pub record_indices: Vec<usize>,
}

#[derive(Debug, Clone, Copy)]
pub enum Network {
    GPS,
    Galileo,
    BeiDou,
    GLONASS,
    Unknown,
}

#[derive(Debug, Clone, Copy)]
pub enum Band {
    L1,
    L5,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct Record {
    pub id: u32,
    pub satellite: u32,
    pub elevation: f64,
    pub azimuth: f64,
    pub snr: f64,
    pub time: i64,
    pub network: Network,
    pub band: Band,
}

pub struct Config {
    pub min_elevation: f64,
    pub max_elevation: f64,
    pub min_azimuth: f64,
    pub max_azimuth: f64,
    pub min_height: f64,
    pub max_height: f64,
    pub step_size: f64,
}