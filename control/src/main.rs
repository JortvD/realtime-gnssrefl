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
use embassy_rp::uart;
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

use crate::compute::run_compute;
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

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("Start of Control");
    // Init peripherals

    let mut config: embassy_rp::config::Config = Default::default();
    config.clocks = ClockConfig::system_freq(150_000_000).unwrap();
    let p = embassy_rp::init(config);

    let config = types::Config::default();
    let mut nmea_parser = nmea::NMEAParser::new(config);

    let sys_freq = clk_sys_freq();
    info!("System clock frequency: {} MHz", sys_freq / 1_000_000);
    
    // GPIOS
    let mut led: Output<'_> = Output::new(p.PIN_25, Level::Low);

    // UART GPS
    let mut config_uart_gps = uart::Config::default();
    config_uart_gps.baudrate = 921_600;
    let uart_gps = uart::Uart::new(p.UART0, p.PIN_16, p.PIN_17, Irqs, p.DMA_CH0, p.DMA_CH1, config_uart_gps);

    let storage = FlashStorage::new(p.FLASH, false);
    let bin_storage = storage::BinStorage::new(50, 240, storage);

    let sector = Sector::new(0, 45602, 30, 1, 240, 50);


    // info!("Data: {:?}", &data2[..]);

    // Spawn tasks
    // spawner.spawn(run_measure(uart_gps, nmea_parser, sector, bin_storage)).unwrap();
    spawner.spawn(run_compute(sector, bin_storage)).unwrap();
    spawner.spawn(led_blink(led)).unwrap();
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