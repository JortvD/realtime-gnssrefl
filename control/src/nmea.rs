use core::str::Split;

use embassy_time::Duration;
use heapless::Vec;
use crate::{types::*, utils};

pub const BURST_SAT_SIZE: usize = 63;

const NETWORK_BITS: usize = 2;
const ELEVATION_BITS: usize = 7;
const AZIMUTH_BITS: usize = 9;
const SNR_BITS: usize = 6;
const BAND_BITS: usize = 1;

const NUM_BITS: usize = 8;

pub struct NmeaBurst {
    pub lines: Vec<Nmealine, NMEA_MAX_LINES>,
    pub bytes: u32,
    pub duration: Duration,
}

impl NmeaBurst {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            bytes: 0,
            duration: Duration::from_millis(0),
        }
    }

    pub fn push(&mut self, line: Nmealine) {
        self.lines.push(line).ok();
    }

    pub fn last(&self) -> Option<&Nmealine> {
        self.lines.last()
    }
}

pub struct Burst {
    pub time: u32,
    pub lat: f32,
    pub lon: f32,
    pub date: Option<u32>,
    pub num: u32,
    pub samples: Vec<Sample, BURST_SAT_SIZE>,
}

impl Burst {
    pub fn new() -> Self {
        Self {
            time: 0,
            lat: 0.0,
            lon: 0.0,
            date: None,
            num: 0,
            samples: Vec::new(),
        }
    }

    pub fn push(&mut self, sample: Sample) -> Result<(), ()> {
        self.samples.push(sample).map_err(|_| ())
    }
    
    pub fn to_bytes(&self) -> Vec<u8, {4 * BURST_SIZE}> {
        let mut data = Vec::<u8, {4 * BURST_SIZE}>::new();

        for sample in self.samples.iter() {
            let byte0 = (*sample & 0xFF) as u8;
            let byte1 = ((*sample >> 8) & 0xFF) as u8;
            let byte2 = ((*sample >> 16) & 0xFF) as u8;
            let byte3 = ((*sample >> 24) & 0xFF) as u8;
            data.push(byte0).unwrap();
            data.push(byte1).unwrap();
            data.push(byte2).unwrap();
            data.push(byte3).unwrap();
        }

        data
    }
}

pub struct NMEAParser {
    config: Option<Config>,
}

impl NMEAParser {
    pub fn new() -> Self {
        Self { config: None }
    }

    pub fn new_from_config(config: &Config) -> Self {
        Self { config: Some(config.clone()) }
    }

    pub fn parse_burst(&mut self, nmeaburst: &NmeaBurst, add_date: bool) -> Burst {
        let mut burst = Burst::new();
        burst.time = u32::MAX;
        let mut num: u32 = 0;

        burst.push(0).ok();

        for line in &nmeaburst.lines {
            // Split only by ',' to match the expected type for parse_gga
            let mut it = line.split(',');

            let command = match it.next() {
                Some(word) => word,
                None => "",
            };

            if self.is_command(command, "GGA") {
                match self.parse_gga(it) {
                    Some((t, lat, lon)) => {
                        burst.lat = lat;
                        burst.lon = lon;
                        burst.time = t;
                    },
                    None => continue,
                }
            }
            else if self.is_command(command, "GSV") {
                self.parse_gsv(it, command, &mut burst.samples, &mut num);
            }
            else if self.is_command(command, "RMC") && add_date {
                burst.date = self.parse_rmc(it);
            }
        }

        let mut header = burst.time;
        header <<= NUM_BITS;
        header += num;
        burst.samples[0] = header;
        burst.num = num;

        burst
    }

    fn parse_rmc(&mut self, mut it: Split<'_, char>) -> Option<u32> {
        let _time = it.next()?;
        let _status = it.next()?;
        let _lat = it.next()?;
        let _northsouth = it.next()?;
        let _lon = it.next()?;
        let _eastwest = it.next()?;
        let _speed = it.next()?;
        let _course = it.next()?;
        let date_str = it.next()?;

        let day = date_str[0..2].parse::<u16>().ok()?;
        let month = date_str[2..4].parse::<u16>().ok()?;
        let year = date_str[4..6].parse::<u16>().ok()? + 2000;

        Some(utils::days_from_civil(year as i32, month as u32, day as u32) as u32)
    }

    fn parse_gga<'a>(&mut self, mut it: Split<'a, char>) -> Option<(u32, f32, f32)> {
        let time_str = it.next()?;
        let hours = time_str[0..2].parse::<u32>().ok()?;
        let minutes = time_str[2..4].parse::<u32>().ok()?;
        let seconds = time_str[4..6].parse::<u32>().ok()?;

        let lat_str = it.next()?;
        let northsouth_str = it.next()?;
        let lon_str = it.next()?;
        let eastwest_str = it.next()?;

        let lat = utils::parse_lat(lat_str, northsouth_str)?;
        let lon = utils::parse_lon(lon_str, eastwest_str)?;

        Some((hours * 3600 + minutes * 60 + seconds, lat, lon))
    }

    fn command_to_network(&mut self, command: &str) -> Option<u32> {
        if command.len() < 3 {
            return None;
        }
        return match &command[1..3] {
            "GP" => Some(0),
            "GA" => Some(1),
            "GB" => Some(2),
            "GL" => Some(3),
            _ => None,
        };
    }

    fn parse_word_as_u32(&mut self, word: Option<&str>) -> Option<u32> {
        match word {
            Some(w) => w.parse::<u32>().ok(),
            None => None,
        }
    }

    fn fix_sat(&mut self, sat: u32, network: u32) -> u32 {
        match network {
            3 => sat - 64,
            0 => if sat > 192 { sat - 128 } else { sat },
            _ => sat
        }
    }

    fn number_to_band(&mut self, num: u32) -> u32 {
        match num {
            1 => 0,
            5 => 1,
            7 => 1,
            8 => 1,
            _ => 0,
        }
    }

    fn is_command(&self, word: &str, command: &str) -> bool {
        word.starts_with('$') && word.len() == 6 && &word[3..6] == command
    }

    fn parse_gsv<'a>(&mut self, mut it: Split<'a, char>, command: &str, values: &mut Vec<Sample, BURST_SAT_SIZE>, num: &mut u32) -> Option<()> {
        let network = match self.command_to_network(command) {
            Some(t) => t,
            None => return None
        };
        let num_msgs = self.parse_word_as_u32(it.next())?;
        let msg_idx = self.parse_word_as_u32(it.next())?;
        let num_sats = self.parse_word_as_u32(it.next())?;

        let num_recs = if msg_idx == num_msgs {
            num_sats - (num_msgs - 1) * 4
        } else { 4 };

        let mut new_values = Vec::<u32, 4>::new();

        for _ in 0..num_recs {
            let mut value: u32 = 0;

            let sat = match it.next()?.parse::<u32>() {
                Ok(v) => v,
                Err(_) => {
                    it.next(); it.next(); it.next();
                    continue;
                }
            };

            // 7 bits
            value += self.fix_sat(sat, network);

            value <<= NETWORK_BITS;
            value += network;
            
            let elev = match it.next()?.parse::<u32>() {
                Ok(v) => v,
                Err(_) => {
                    it.next(); it.next();
                    continue;
                }
            };

            value <<= ELEVATION_BITS;
            value += elev;

            let azim = match it.next()?.parse::<u32>() {
                Ok(v) => v,
                Err(_) => {
                    it.next();
                    continue;
                }
            };

            value <<= AZIMUTH_BITS;
            value += azim;

            let snr = match it.next()?.parse::<u32>() {
                Ok(v) => v,
                Err(_) => {
                    continue;
                }
            };

            value <<= SNR_BITS;
            value += snr;

            if let Some(config) = &self.config {
                if  elev < config.pre_min_elevation || 
                    elev > config.pre_max_elevation ||
                    (config.pre_min_elevation > config.pre_max_elevation 
                        && (azim < config.pre_min_azimuth || 
                        azim > config.pre_max_azimuth)) ||
                    (config.pre_min_elevation <= config.pre_max_elevation 
                        && (azim < config.pre_min_azimuth && 
                        azim > config.pre_max_azimuth)) {
                    continue;
                }
            }

            new_values.push(value).ok();
        }

        let band = match it.next()?[..1].parse::<u32>() {
            Ok(v) => self.number_to_band(v),
            Err(_) => return None,
        };

        for value in new_values.iter_mut() {
            *value <<= BAND_BITS;
            *value += band;

            if *num >= BURST_SAT_SIZE as u32 {
                return None;
            }

            values.push(*value).ok();
            *num += 1;
        }

        None
    }
}

