use heapless::{Vec, String};

pub const MAX_LINES: usize = 100;
pub const LINE_LEN: usize = 256;

pub type Line = String<LINE_LEN>;
pub type Burst = Vec<Line, MAX_LINES>;

pub struct Config {
    pub pre_min_elevation: u32,
    pub pre_max_elevation: u32,
    pub pre_min_azimuth: u32,
    pub pre_max_azimuth: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            pre_min_elevation: 0,
            pre_max_elevation: 90,
            pre_min_azimuth: 0,
            pre_max_azimuth: 360,
        }
    }
}