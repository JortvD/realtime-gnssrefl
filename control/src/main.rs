//! This example test the RP Pico on board LED.
//!
//! It does not work with the RP Pico W board. See `blinky_wifi.rs`.

#![no_std]
#![no_main]

use core::str::Split;

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::gpio;
use heapless::{String, Vec};
use embassy_time::Timer;
use embassy_rp::uart::{Uart, Config};
use gpio::{Level, Output};
use {defmt_rtt as _, panic_probe as _};

mod nmea;
mod math;

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
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    let data = Vec::<String<128>, 64>::new();

    nmea::parse_burst(data);

    let x = [0.1, 0.2, 0.3];
    let y = [0.4, 0.5, 0.6];
    let frequencies = [1.0, 2.0, 3.0];
    let mut power_out = [0.0; 3];

    math::lombscargle_no_std(&x, &y, &frequencies, &mut power_out);

    let mut chsum = fletcher::Fletcher64::new();
    chsum.update(&[1, 2, 3, 4, 5]);
    let _ = chsum.value();
}