use heapless::{Vec, String};

use crate::{realtime::RealTime, utils};

// Control
pub const MAX_SECTORS: usize = 128;

// NMEA processing
pub const NMEA_MAX_LINES: usize = 64;
pub const NMEA_LINE_LEN: usize = 128;

pub type Nmealine = String<NMEA_LINE_LEN>;
pub type NmeaBurst = Vec<Nmealine,NMEA_MAX_LINES>;

// Storage
pub const FLASH_SIZE: usize = 4 * 1024 * 1024; // 2MB
pub const BLOCK_SIZE: usize = 4096; // 4KB
pub const NUM_CONTAINERS: usize = 51;
pub const NUM_CONTAINER_BLOCKS: usize = 15;
pub const CONTAINER_SIZE: usize = NUM_CONTAINER_BLOCKS * BLOCK_SIZE; // 60KB
pub const USABLE_SIZE: usize = NUM_CONTAINERS * CONTAINER_SIZE; // ~3MB
pub const START_ADDRESS: u32 = 0x100000; // Start at 1MB offset (flash-relative)

// Storage is divided as
// 0-49: bins (50 containers)
pub const NUM_BINS: usize = 50;
pub const BINS_CONTAINER_START: usize = 0;
// 50: measurements (1 container)
pub const NUM_MEASUREMENTS: usize = NUM_CONTAINER_BLOCKS;
pub const MEASUREMENTS_CONTAINER_START: usize = 50;

pub const BURST_SIZE: usize = 64; // Samples per burst
pub const BIN_BURST_SIZE: usize = 240;  // Burst per bin (4 minutes)

pub const MEASUREMENT_STORAGE_SIZE: usize = BLOCK_SIZE; // 4KB

pub const WARMUP_TIME: u32 = 30;

pub type Sample = u32;
pub type Burst = Vec<Sample, BURST_SIZE>;
pub type Bin = Vec<Burst, BIN_BURST_SIZE>;

#[derive(Clone)]
pub struct Config {
    pub pre_min_elevation: u32,
    pub pre_max_elevation: u32,
    pub pre_min_azimuth: u32,
    pub pre_max_azimuth: u32,

    pub post_min_elevation: u32,
    pub post_max_elevation: u32,
    pub post_min_azimuth: u32,
    pub post_max_azimuth: u32,

    pub min_relative_height: f32,
    pub max_relative_height: f32,
    pub relative_height_step_size: f32,

    pub qc_min_elevation_range: u32,
    pub qc_iqr_size: f32,

    pub sector_mid_times: Vec<u32, 24>,

    pub bins_per_sector: u32,
    pub seconds_per_bin: u32,

    pub num_send_measurements: u32,
}

impl Config {
    pub fn get_mid_times_as_str(&self) -> String<128> {
        let mut mid_times_str = String::<128>::new();
        for (idx, &midpoint) in self.sector_mid_times.iter().enumerate() {
            let time_str = utils::seconds_to_time_str(midpoint);
            mid_times_str.push_str(&time_str).unwrap();
            if idx < self.sector_mid_times.len() - 1 {
                mid_times_str.push_str(", ").unwrap();
            }
        }
        mid_times_str
    }

    pub fn get_measure_duration(&self) -> u32 {
        return self.bins_per_sector*self.seconds_per_bin;
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut mid_times = Vec::<u32, 24>::new();
        mid_times.push(utils::time_str_to_seconds("01:00:00").unwrap()).unwrap();
        mid_times.push(utils::time_str_to_seconds("03:00:00").unwrap()).unwrap();
        mid_times.push(utils::time_str_to_seconds("05:00:00").unwrap()).unwrap();
        mid_times.push(utils::time_str_to_seconds("07:00:00").unwrap()).unwrap();
        mid_times.push(utils::time_str_to_seconds("09:00:00").unwrap()).unwrap();
        mid_times.push(utils::time_str_to_seconds("11:00:00").unwrap()).unwrap();
        mid_times.push(utils::time_str_to_seconds("13:00:00").unwrap()).unwrap();
        mid_times.push(utils::time_str_to_seconds("15:00:00").unwrap()).unwrap();
        mid_times.push(utils::time_str_to_seconds("17:00:00").unwrap()).unwrap();
        mid_times.push(utils::time_str_to_seconds("19:00:00").unwrap()).unwrap();
        mid_times.push(utils::time_str_to_seconds("21:00:00").unwrap()).unwrap();
        mid_times.push(utils::time_str_to_seconds("23:00:00").unwrap()).unwrap();
        Self {
            pre_min_elevation: 10,
            pre_max_elevation: 30,
            pre_min_azimuth: 50,
            pre_max_azimuth: 150,

            post_min_elevation: 10,
            post_max_elevation: 30,
            post_min_azimuth: 50,
            post_max_azimuth: 150,

            min_relative_height: 0.5,
            max_relative_height: 10.0,
            relative_height_step_size: 0.5,

            qc_min_elevation_range: 10,
            qc_iqr_size: 1.5,

            sector_mid_times: mid_times,

            bins_per_sector: 15,
            seconds_per_bin: 240,

            num_send_measurements: 3,
        }
    }
}

# [derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectorState {
    AWAITING,
    TO_MEASURE,
    MEASURING,
    TO_COMPUTE,
    COMPUTING,
    TO_COMMUNICATE,
    COMMUNICATING,
    DONE
}

#[derive(Clone, Debug)]
pub struct Sector {
    uid: u32,

    midpoint_index: u32,
    measurement_index: u32,
    start_bin_index: u32,
    start_time: u32,
    n_bins: u32,

    seconds_per_bin: u32,
    pub state: SectorState,
}

impl Sector {
    pub fn new(uid: u32, midpoint_index: u32, measurement_index: u32, start_bin_index: u32, start_time: u32, n_bins: u32, seconds_per_bin: u32, state: SectorState) -> Self {
        Self {
            uid,
            midpoint_index,
            measurement_index,
            start_bin_index,
            start_time,
            n_bins,
            seconds_per_bin,
            state,
        }
    }
    pub fn get_uid(&self) -> u32 {
        self.uid
    }

    pub fn get_midpoint_index(&self) -> u32 {
        self.midpoint_index
    }

    pub fn get_measurement_index(&self) -> u32 {
        self.measurement_index
    }

    pub fn get_start_bin_index(&self) -> u32 {
        self.start_bin_index
    }

    pub fn get_start_time(&self) -> u32 {
        self.start_time
    }

    pub fn get_end_time(&self) -> u32 {
        self.start_time + self.n_bins * self.seconds_per_bin
    }

    pub fn get_bins(&self) -> Vec<u32, 128> {
        let mut bins = Vec::<u32, 128>::new();
        for i in 0..self.n_bins {
            bins.push((self.start_bin_index + i) % NUM_BINS as u32).unwrap();
        }
        bins
    }

    pub fn get_bin_for_time(&self, time: u32) -> u32 {
        let dt = time - self.start_time;
        let bin_id = self.start_bin_index + dt / self.seconds_per_bin;

        bin_id % NUM_BINS as u32
    }

    pub fn is_time_before_sector(&self, time: u32) -> bool {
        time < self.get_start_time()
    }

    pub fn is_time_after_sector(&self, time: u32) -> bool {
        time >= self.get_end_time()
    }

    pub fn is_succeeding(&self, previous: &Sector) -> bool {
        RealTime::diff_wrapping(self.start_time, previous.get_end_time()) < 30 // TODO remove magic number
    }
}

pub const MAX_MEASUREMENT_OBSERVATIONS: usize = 128;

pub const MEASUREMENT_HEADER_SIZE: usize = 20;
pub const OBSERVATION_SIZE: usize = 28;

pub const MEASUREMENT_SIZE: usize = MEASUREMENT_HEADER_SIZE + (OBSERVATION_SIZE * MAX_MEASUREMENT_OBSERVATIONS);

pub struct Measurement {
    pub num_seen: u32,
    pub observations: Vec<Observation, MAX_MEASUREMENT_OBSERVATIONS>,
    pub start_time: u32,
    pub end_time: u32,
    pub mean: f32,
    pub std: f32,
}


impl Measurement {
    pub fn new(num_seen: u32, start_time: u32, end_time: u32, mean: f32, std: f32) -> Self {
        Self {
            num_seen,
            observations: Vec::new(),
            start_time,
            end_time,
            mean,
            std,
        }
    }

    pub fn from_bytes(data: &[u8; MEASUREMENT_SIZE]) -> Self {
        let start_time = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let end_time = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let num_seen = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        let mean = f32::from_le_bytes([data[12], data[13], data[14], data[15]]);
        let std = f32::from_le_bytes([data[16], data[17], data[18], data[19]]);
        // info!("Header: {:?} -> {},{},{},{},{}", &data[0..20], start_time, end_time, num_seen, mean, std);
        let mut observations = Vec::<Observation, MAX_MEASUREMENT_OBSERVATIONS>::new();

        for i in 0..MAX_MEASUREMENT_OBSERVATIONS {
            let offset = MEASUREMENT_HEADER_SIZE + i * OBSERVATION_SIZE;
            let obs_data: &[u8; 28] = data[offset..offset + 28].try_into().unwrap();
            let observation = Observation::from_bytes(obs_data);
            if observation.used {
                observations.push(observation).ok();
            }
        }

        Self {
            num_seen,
            observations,
            start_time,
            end_time,
            mean,
            std,
        }
    }

    pub fn push(&mut self, obs: Observation) {
        self.observations.push(obs).ok();
    }

    pub fn to_bytes(&self) -> [u8; MEASUREMENT_SIZE] {
        let mut data = [0u8; MEASUREMENT_SIZE];
        data[0..4].copy_from_slice(&self.start_time.to_le_bytes());
        data[4..8].copy_from_slice(&self.end_time.to_le_bytes());
        data[8..12].copy_from_slice(&self.num_seen.to_le_bytes());
        data[12..16].copy_from_slice(&self.mean.to_le_bytes());
        data[16..20].copy_from_slice(&self.std.to_le_bytes());
        // info!("Header: {},{},{},{},{} -> {:?}", self.start_time, self.end_time, self.num_seen, self.mean, self.std, &data[0..20]);

        for (i, obs) in self.observations.iter().enumerate() {
            let obs_bytes = obs.to_bytes();
            data[MEASUREMENT_HEADER_SIZE + i * OBSERVATION_SIZE..MEASUREMENT_HEADER_SIZE + (i + 1) * OBSERVATION_SIZE].copy_from_slice(&obs_bytes);
        }

        data
    }
}

pub struct Observation {
    pub sat_id: u16,
    pub start_time: u16,
    pub end_time: u16,
    pub max_amp: f32,
    pub max_rh: f32,
    pub mean_amp: f32,
    pub num_recs: u32,
    pub used: bool,
}

impl Observation {
    pub fn from_bytes(data: &[u8; 28]) -> Self {
        let sat_id = u16::from_le_bytes([data[0], data[1]]);
        let start_time = u16::from_le_bytes([data[2], data[3]]);
        let end_time = u16::from_le_bytes([data[4], data[5]]);
        let max_amp = f32::from_le_bytes([data[6], data[7], data[8], data[9]]);
        let max_rh = f32::from_le_bytes([data[10], data[11], data[12], data[13]]);
        let mean_amp = f32::from_le_bytes([data[14], data[15], data[16], data[17]]);
        let num_recs = u32::from_le_bytes([data[18], data[19], data[20], data[21]]);
        let used = data[22] != 0;

        Self {
            sat_id,
            start_time,
            end_time,
            max_amp,
            max_rh,
            mean_amp,
            num_recs,
            used,
        }
    }

    #[inline]
    pub fn peak_to_mean(&self) -> f32 {
        if self.mean_amp > 0.0 {
            self.max_amp / self.mean_amp
        } else {
            0.0
        }
    }

    pub fn to_bytes(&self) -> [u8; 28] {
        let mut data = [0u8; 28];
        data[0..2].copy_from_slice(&self.sat_id.to_le_bytes());
        data[2..4].copy_from_slice(&self.start_time.to_le_bytes());
        data[4..6].copy_from_slice(&self.end_time.to_le_bytes());
        data[6..10].copy_from_slice(&self.max_amp.to_le_bytes());
        data[10..14].copy_from_slice(&self.max_rh.to_le_bytes());
        data[14..18].copy_from_slice(&self.mean_amp.to_le_bytes());
        data[18..22].copy_from_slice(&self.num_recs.to_le_bytes());
        data[22] = if self.used { 1 } else { 0 };
        data
    }
}