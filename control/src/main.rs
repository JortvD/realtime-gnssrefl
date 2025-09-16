//! This example test the RP Pico on board LED.
//!
//! It does not work with the RP Pico W board. See `blinky_wifi.rs`.

#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::gpio;
use embassy_rp::uart;
use embassy_time::Timer;
use gpio::{Level, Output};
use {defmt_rtt as _, panic_probe as _};

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
    // Init peripherals
    let p = embassy_rp::init(Default::default());

    //GPIOS
    let led: Output<'_> = Output::new(p.PIN_25, Level::Low);

    // UART1
    let config = uart::Config::default();
    let uart: uart::Uart<'_, uart::Blocking> = uart::Uart::new_blocking(p.UART1, p.PIN_4, p.PIN_5, config);

    spawner.spawn(uart_heartbeat(uart)).unwrap();
    spawner.spawn(led_blink(led)).unwrap();
}

#[embassy_executor::task]
async fn uart_heartbeat(mut uart: uart::Uart<'static, uart::Blocking>) {
    loop {
        uart.blocking_write("Hello there \r\n".as_bytes()).unwrap();
        Timer::after_millis(1000).await;
    }
}

#[embassy_executor::task]
async fn led_blink(mut led: Output<'static>) {
    loop {
        led.set_high();
        Timer::after_millis(500).await;
        led.set_low();
        Timer::after_millis(500).await;
    }
}