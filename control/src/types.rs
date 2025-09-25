use heapless::{Vec, String};

// NMEA processing
pub const NMEA_MAX_LINES: usize = 100;
pub const NMEA_LINE_LEN: usize = 256;

pub type Nmealine = String<NMEA_LINE_LEN>;
pub type NmeaBurst = Vec<Nmealine,NMEA_MAX_LINES>;

// Storage
pub const BURST_SAMPLE_SIZE: usize = 64; // Samples per burst
pub const BIN_BURST_SIZE: usize = 240;  // Burst per bin (4 minutes)

pub type Sample = u32;
pub type Burst = Vec<Sample, BURST_SAMPLE_SIZE>;
pub type Bin = Vec<Burst, BIN_BURST_SIZE>;

pub struct Config {
    pub pre_min_elevation: u32,
    pub pre_max_elevation: u32,
    pub pre_min_azimuth: u32,
    pub pre_max_azimuth: u32,

    pub sector_measure_duration: u32,
    pub sector_midpoints: Vec<u32, 24>,
}

impl Default for Config {
    fn default() -> Self {
        let mut midpoints = Vec::<u32, 24>::new();
        midpoints.push(5).unwrap();
        midpoints.push(11).unwrap();
        midpoints.push(17).unwrap();
        midpoints.push(23).unwrap();
        Self {
            pre_min_elevation: 10,
            pre_max_elevation: 30,
            pre_min_azimuth: 50,
            pre_max_azimuth: 150,
            sector_measure_duration: 60 * 60 * 2,
            sector_midpoints: midpoints,
        }
    }
}

pub struct Sector {
    pub start_bin_id: u32,
    start_time: u32,
    n_bins: u32,
    pub measure_interval: u32,

    bin_time_size: u32,
    total_bins: u32,
}

impl Sector {
    pub fn new(start_bin_id: u32, start_time: u32, n_bins: u32, measure_interval: u32, bin_time_size: u32, total_bins: u32) -> Self {
        Self {
            start_bin_id,
            start_time,
            n_bins,
            measure_interval,
            bin_time_size,
            total_bins,
        }
    }

    pub fn get_bins(&self) -> Vec<u32, 128> {
        let mut bins = Vec::<u32, 128>::new();
        for i in 0..self.n_bins {
            bins.push((self.start_bin_id + i) % self.total_bins).unwrap();
        }
        bins
    }

    pub fn get_bin_for_time(&self, time: u32) -> u32 {
        let dt = time - self.start_time;
        let bin_id = self.start_bin_id + dt / self.bin_time_size;

        bin_id % self.total_bins
    }

    pub fn is_time_before_sector(&self, time: u32) -> bool {
        time < self.start_time
    }

    pub fn is_time_after_sector(&self, time: u32) -> bool {
        time > self.start_time + self.n_bins * self.bin_time_size
    }
}