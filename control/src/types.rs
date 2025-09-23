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
