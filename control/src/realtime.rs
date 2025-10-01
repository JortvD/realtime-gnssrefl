use defmt::info;
use embassy_time::Instant;
use embassy_time::Timer;

use crate::utils;

pub const SPEEDUP_FACTOR: u64 = 10;
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

    pub fn update(&mut self, deviation: Deviation, date: u32) {
        let deviation_ms = Instant::now().as_secs() - deviation.measured_at.as_secs();
        self.ref_real_time = deviation.time + (deviation_ms / 1000) as u32;
        self.ref_real_date = date;
        self.ref_pico_time = Instant::from_secs(Instant::now().as_secs()*SPEEDUP_FACTOR);
        info!(
            "[cont] updated reference time to {} ({}) and date to {} ({}) (pico at {} ms since startup). Update delay {} ms", 
            self.ref_real_time, 
            utils::seconds_to_time_str(self.ref_real_time).as_str(), 
            self.ref_real_date, utils::date_to_str(self.ref_real_date).as_str(), 
            self.ref_pico_time.as_millis(),
            deviation_ms
        );
    }

    pub fn next_or_first(vec: &[u32], x: u32) -> (u32, usize) {
        vec.iter()
            .enumerate()
            .find(|&(_, &v)| v > x)
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

    pub fn get_timer(sleeptime: u32) -> Timer{
        Timer::after_millis(sleeptime as u64 * 1000 / SPEEDUP_FACTOR)
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