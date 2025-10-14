use defmt::info;
use embassy_time::Instant;
use embassy_time::Timer;

use crate::utils;

pub const SPEEDUP_FACTOR: u64 = 1;
const SECONDS_PER_DAY: u32 = 86400;

pub struct RealTime {
    ref_real_date: u32,
    ref_real_time: u32,
    ref_pico_time: Instant
}

impl RealTime {
    pub fn new() -> Self{
        Self {
            ref_real_date: 0,
            ref_real_time: 0,
            ref_pico_time: Instant::from_secs(Instant::now().as_secs()*SPEEDUP_FACTOR)
        }
    }

    pub fn get_real_time(&self) -> u32 {
        let time = self.ref_real_time + Instant::from_secs(Instant::now().as_secs()*SPEEDUP_FACTOR)
        .duration_since(self.ref_pico_time).as_secs() as u32;
        (time % 86400) as u32
    }

    pub fn get_days(&self) -> u32 {
        return self.ref_real_date;
    }

    pub fn update_date(&mut self, date: u32) {
        info!(
            "[time] updated reference date from {} to ({}) to {} ({})", 
            self.ref_real_date, utils::date_to_str(self.ref_real_date).as_str(),
            date, utils::date_to_str(date).as_str()
        );
        self.ref_real_date = date;
    }

    pub fn update_time(&mut self, deviation: Deviation) {
        let old_time = self.ref_real_time;
        let deviation_ms = Instant::now().as_millis() - deviation.measured_at.as_millis();
        self.ref_real_time = deviation.time + (deviation_ms / 1000) as u32;
        self.ref_pico_time = Instant::from_secs(Instant::now().as_secs()*SPEEDUP_FACTOR);
        info!(
            "[time] updated reference time from {} to ({}) to {} ({}) (pico at {} s since startup). Update delay {} ms", 
            old_time,
            utils::seconds_to_time_str(old_time).as_str(),
            self.ref_real_time, 
            utils::seconds_to_time_str(self.ref_real_time).as_str(),
            self.ref_pico_time.as_secs(),
            deviation_ms
        );
    }

pub fn next_or_first(vec: &[u32], x: u32) -> (u32, usize) {
    vec.iter()
        .enumerate()
        .filter(|&(_, &v)| v > x)
        .min_by_key(|&(_, v)| v)   
        .map(|(i, &v)| (v, i))
        .unwrap_or((vec[0], 0))  
}


    // Assumes one measurement per day
    pub fn subtract_wrapping(t1: u32, t2: u32) -> u32 {
        (t1 - t2 + SECONDS_PER_DAY) % SECONDS_PER_DAY
    }

    pub fn add_wrapping(t1: u32, t2: u32) -> u32 {
        (t1 + t2) % SECONDS_PER_DAY
    }

    pub fn diff_wrapping(t1: u32, t2: u32) -> u32 {
        let diff = if t1 >= t2 {
            t1 - t2
        } else {
            t2 - t1
        };
        let wrapped_diff = SECONDS_PER_DAY - diff;
        diff.min(wrapped_diff)
    }
    
    pub fn get_timer(&self, sleep_until: u32) -> Timer {
        let now = self.get_real_time();
        Timer::after_millis(RealTime::subtract_wrapping(sleep_until, now) as u64 * 1000 / SPEEDUP_FACTOR)
    }
}

pub struct Deviation {
    time: u32,
    measured_at: Instant,
}

impl Deviation {
    pub fn new(time: u32, measured_at: Instant) -> Self {
        Self {
            time,
            measured_at,
        }
    }
}