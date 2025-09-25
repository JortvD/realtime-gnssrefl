
use embassy_time::Duration;

use embassy_rp::uart::{Uart, Async, Config};
use embassy_rp::gpio::Output;
use embassy_executor::Spawner;
use embassy_time::{Instant, Timer};

use crate::gnss::GNSSSensor;
use crate::measure::run_measure;
use crate::storage::{BinStorage, FlashStorage};
use crate::StorageType;
use crate::types::{self, Sector};
use crate::nmea::NMEAParser;
use heapless::Vec;

#[embassy_executor::task]
pub async fn core0_task_control(
    spawner: Spawner,
    gnss_sensor: GNSSSensor,
    uart_rockblock: Uart<'static, Async>,
    led: Output<'static>,
    storage: &'static StorageType) 
    {

    let config = types::Config::default();
    let mut nmea_parser = NMEAParser::new(&config);
    let config = types::Config::default();

    let realtime = RealTime::new();

    // Update clock with real current time
    //realtime.update_ref_time(0);

    loop {
        let next_midpoint = next_or_min(&config.sector_midpoints, realtime.get_real_time());
        let next_startpoint = next_midpoint - &config.sector_measure_duration / 2;
        
        let start_bin_id = 0;
        let n_bins = config.sector_measure_duration / config.seconds_per_bin;
        
        let sleeptime = next_startpoint - realtime.get_real_time();
        let timer_future = Timer::after_secs(sleeptime as u64);
        //Use select to also wait for channel
        timer_future.await;
        //spawner.spawn(run_measure(uart_gps, nmea_parser, sector, binstorage));
    }


    //spawner.spawn(run_measure(uart_gps, nmea_parser, sector, bin_storage)).unwrap();
    // spawner.spawn(run_compute(sector, bin_storage)).unwrap();
    // spawner.spawn(clock::clk_control()).unwrap();
    // spawner.spawn(led_blink(led)).unwrap();
}

#[embassy_executor::task]
pub async fn core1_task_control(
    spawner: Spawner,) 
    {

    //spawner.spawn(run_measure(uart_gps, nmea_parser, sector, bin_storage)).unwrap();
    // spawner.spawn(run_compute(sector, bin_storage)).unwrap();
    // spawner.spawn(clock::clk_control()).unwrap();
    // spawner.spawn(led_blink(led)).unwrap();
}

struct RealTime {
    ref_real_time: u32,
    ref_pico_time: Instant
}

impl RealTime {
    pub fn new() -> Self{
        Self {
           ref_real_time: 0,
           ref_pico_time: Instant::now(),
        }
    }

    fn get_real_time(&self) -> u32 {
        let time = self.ref_real_time + Instant::now().duration_since(self.ref_pico_time).as_secs() as u32;
        (time % 86400) as u32
    }

    // fn convert_to_pico_time(self, real_time: u32) -> Instant {
    //     real_time - self.ref_real_time 

    // }

    fn update_ref_time(&mut self, ref_time: u32) {
        self.ref_real_time = ref_time;
        self.ref_pico_time = Instant::now();
    }
}




fn next_or_min(vec: &Vec<u32, 24>, x: u32) -> u32 {
    // Try to find the smallest value bigger than x
    vec.iter()
        .filter(|&&v| v > x)
        .min()
        // fallback: if none found, return overall smallest
        .copied()
        .or_else(|| vec.iter().min().copied()).unwrap()
}