//! This example test the RP Pico on board LED.
//!
//! It does not work with the RP Pico W board. See `blinky_wifi.rs`.

#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::clocks::clk_sys_freq;
use embassy_rp::clocks::ClockConfig;
use embassy_rp::gpio;
use embassy_rp::multicore::pause_core1;
use embassy_rp::uart;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Instant, Timer};
use embassy_rp::uart::{Uart, Config};
use gpio::{Level, Output};
use embassy_rp::bind_interrupts;
use embassy_rp::uart::InterruptHandler as UARTInterruptHandler;
use embassy_rp::peripherals::UART0;
use heapless::Vec;

use {defmt_rtt as _, panic_probe as _};

mod nmea;
mod math;
mod storage;
mod types;
mod measure;
mod compute;
mod clock;
mod utils;
mod get_time;
mod gnss;
mod comms;
mod rockblock;

use crate::comms::run_comms;
use crate::compute::run_compute;
use crate::get_time::get_time;
use crate::gnss::GNSSSensor;
use crate::measure::run_measure;
use crate::storage::FlashStorage;
use crate::types::*;

bind_interrupts!(pub struct Irqs {
    UART0_IRQ  => UARTInterruptHandler<UART0>;
});


// Program metadata for `picotool info`.
// This isn't needed, but it's recomended to have these minimal entries.
#[unsafe(link_section = ".bi_entries")]
#[used]
pub static PICOTOOL_ENTRIES: [embassy_rp::binary_info::EntryAddr; 4] = [
    embassy_rp::binary_info::rp_program_name!(c"Blinky Example"),
    embassy_rp::binary_info::rp_program_description!(
        c"This example tests the RP Pico on board LED, connected to gpio 25"
    ),
    embassy_rp::binary_info::rp_cargo_version!(),
    embassy_rp::binary_info::rp_program_build_attribute!(),
];

type StorageType = Mutex<ThreadModeRawMutex, Option<FlashStorage>>;
static STORAGE: StorageType = Mutex::new(None);

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("Start of Control");
    // Init peripherals

    let mut config: embassy_rp::config::Config = Default::default();
    config.clocks = ClockConfig::system_freq(150_000_000).unwrap();
    let p = embassy_rp::init(config);

    let config = types::Config::default();

    let sys_freq = clk_sys_freq();
    info!("System clock frequency: {} MHz", sys_freq / 1_000_000);
    
    // GPIOS
    let mut led: Output<'_> = Output::new(p.PIN_25, Level::Low);

    // UART GNSS
    let mut gnss_uart_config = uart::Config::default();
    gnss_uart_config.baudrate = 921_600;
    let gnss_uart = uart::Uart::new(p.UART0, p.PIN_16, p.PIN_17, Irqs, p.DMA_CH0, p.DMA_CH1, gnss_uart_config);
    let mut gnss_sensor = GNSSSensor::new(gnss_uart, &config);

    let storage = FlashStorage::new(p.FLASH, false);

    {
        *(STORAGE.lock().await) = Some(storage);
    }

    let sector = Sector::new(0, 0, 45602, config.bins_per_sector, config.seconds_per_bin);

    let mut rockblock = rockblock::RockBlock::new();

    // let time = get_time(&mut gnss_sensor).await;
    // info!("Current time is {}", time);

    // pause_core1();

    // Spawn tasks
    // spawner.spawn(run_measure(gnss_sensor, sector, bin_storage)).unwrap();
    spawner.spawn(run_compute(sector, &STORAGE, config)).unwrap();
    spawner.spawn(run_comms(rockblock, &STORAGE, 0, 1)).unwrap();
    // spawner.spawn(clock::clk_control()).unwrap();
    // spawner.spawn(led_blink(led)).unwrap();
}


#[embassy_executor::task]
async fn led_blink(mut led: Output<'static>) {
    loop {
        led.set_high();
        Timer::after_millis(25).await;
        led.set_low();
        Timer::after_millis(25).await;
    }
}