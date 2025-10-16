use crate::realtime::*;
use crate::types::*;
use heapless::Vec;
use defmt::info;
use crate::utils;

pub struct Scheduler {}

impl Scheduler {
    pub fn new() -> Self {
        Self {}
    }

    pub fn get_next_sector(&self, config: &Config, realtime: &RealTime, sector: &Sector) -> (Sector, u32, u32) {
        let now_time = realtime.get_real_time();
        let now_days = realtime.get_days();
        let midpoint_index = self.get_next_sector_midpoint_index(&config, &sector);
        self.get_sectortimer_from_midpoint_index(&config, midpoint_index, now_time, now_days)
    }

    pub fn get_first_sector(&self, config: &Config, realtime: &RealTime) -> (Sector, u32, u32) {
        let now_time = realtime.get_real_time();
        let now_days = realtime.get_days();
        let midpoint_index = self.get_first_sector_midpoint_index(config, now_time);
        self.get_sectortimer_from_midpoint_index(&config, midpoint_index, now_time, now_days)
    }

    fn get_sectortimer_from_midpoint_index(&self, config: &Config, midpoint_index: u32, now_time: u32, now_days: u32) -> (Sector, u32, u32) {
        let sectors_per_day = config.sector_mid_times.len() as u32;
        let half_measure_duration = config.get_measure_duration() / 2;

        let mid_time = config.sector_mid_times.get(midpoint_index as usize).unwrap().clone();
        let start_time = RealTime::subtract_wrapping(mid_time, half_measure_duration);

        let is_next_day = start_time <= now_time;
        let sector_day = if is_next_day { now_days + 1 } else { now_days };

        // Compute uid
        let uid = sector_day * sectors_per_day + midpoint_index;

        // Compute measurement index
        let measurement_index = uid % NUM_MEASUREMENTS as u32;

        // Compute bin range
        let start_bin_index = (uid * config.bins_per_sector) % NUM_BINS as u32;
        let n_bins = config.bins_per_sector;

        // Create Sector
        let sector = Sector::new(
            uid,
            midpoint_index,
            measurement_index as u32,
            start_bin_index,
            start_time,
            n_bins,
            config.seconds_per_bin,
            SectorState::AWAITING
        );

        info!(
            "[cont] next sector {} starts at {} s ({}) on day {} ({}) with bins {:?}", 
            sector.get_measurement_index(), 
            sector.get_start_time(), 
            utils::seconds_to_time_str(sector.get_start_time()).as_str(),
            sector_day,
            utils::date_to_str(sector_day).as_str(),
            sector.get_bins().as_slice()
        );

        // Sleep until start of next sector (could be the next day)
        let start_time_with_warmup = RealTime::subtract_wrapping(start_time, WARMUP_TIME);

        info!(
            "[cont] sleeping from {} ({}) to {} s ({}) until next sector starts", 
            now_time,
            utils::seconds_to_time_str(now_time).as_str(),
            start_time_with_warmup, 
            utils::seconds_to_time_str(start_time_with_warmup).as_str()
        );

        (sector, start_time_with_warmup, if !is_next_day { 0 } else { 1 })
    }

    fn get_next_sector_midpoint_index(&self, config: &Config, sector: &Sector) -> u32{
        let sectors_per_day = config.sector_mid_times.len() as u32;

        (sector.get_midpoint_index() + 1) % sectors_per_day
    }

    fn get_first_sector_midpoint_index(&self, config: &Config, now: u32) -> u32 {
        let half_measure_duration = config.get_measure_duration() / 2;
        let sector_start_times: Vec<u32, MAX_MIDPOINTS> = config
            .sector_mid_times
            .iter()
            .map(|&midpoint| RealTime::subtract_wrapping(midpoint , half_measure_duration))
            .collect();
        let start_time_with_warmup = RealTime::add_wrapping(now, WARMUP_TIME);
        let (_start_time, midpoint_index) = RealTime::next_or_first(&sector_start_times, start_time_with_warmup);
        
        midpoint_index as u32
    }

}