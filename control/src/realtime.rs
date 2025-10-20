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

    pub fn get_time_since_update(&self) -> u32 {
        Instant::from_secs(Instant::now().as_secs()*SPEEDUP_FACTOR)
        .duration_since(self.ref_pico_time).as_secs() as u32
    }

    pub fn get_real_time(&self) -> u32 {
        let time = self.ref_real_time + self.get_time_since_update();
        (time % 86400) as u32
    }

    pub fn get_real_date(&self) -> u32 {
        let extra_days = (self.ref_real_time + self.get_time_since_update()) / SECONDS_PER_DAY;
        self.ref_real_date + extra_days
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

pub fn next_or_first(vec: &[u32], x: u32) -> usize {
    vec.iter()
        .enumerate()
        .filter(|&(_, &v)| v > x)
        .min_by_key(|&(_, v)| v)
        .map(|(i, _)| i)
        .unwrap_or(0)
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
    
    pub fn get_timer(&self, sleep_time: u32, sleep_date: u32) -> Timer {
        let now_time = self.get_real_time();
        let now_date = self.get_real_date();

        let mut total_sleep_secs = 0u32;
        
        if sleep_time >= now_time {
            total_sleep_secs += sleep_time - now_time;
        } else {
            total_sleep_secs += SECONDS_PER_DAY - (now_time - sleep_time);
        }

        if sleep_time == now_time && sleep_date > now_date {
            total_sleep_secs += SECONDS_PER_DAY;
        }

        info!(
            "[time] sleeping for {} s to reach time {} ({}) on date {} ({}). Now is {} ({}) on date {} ({})",
            total_sleep_secs,
            sleep_time,
            utils::seconds_to_time_str(sleep_time).as_str(),
            sleep_date,
            utils::date_to_str(sleep_date).as_str(),
            now_time,
            utils::seconds_to_time_str(now_time).as_str(),
            now_date,
            utils::date_to_str(now_date).as_str()
        );
        Timer::after_millis(total_sleep_secs as u64 * 1000 / SPEEDUP_FACTOR)
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