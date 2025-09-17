//! This example test the RP Pico on board LED.
//!
//! It does not work with the RP Pico W board. See `blinky_wifi.rs`.

#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::gpio;
use heapless::Vec;
use embassy_time::Timer;
use embassy_rp::uart::{Uart, Config};
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
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let mut uart = Uart::new_blocking(
        p.UART0,
        p.PIN_0,
        p.PIN_1,
        Default::default(),
    );

    let mut buffer = Vec::<u8, 1024>::new();

    loop {
        let line = readline(&mut uart, &mut buffer);
    }
}

async fn readline(uart: &mut Uart<'static, embassy_rp::uart::Blocking>, buffer: &mut Vec<u8, 1024>) -> Vec<u8, 128> {
    loop {
        // Read up to 32 bytes into a temporary buffer
        let mut temp = [0u8; 32];
        match uart.read_blocking(&mut temp) {
            Ok(n) if n > 0 => {
                // Push read bytes into buffer
                for &b in &temp[..n] {
                    if buffer.push(b).is_err() {
                        // Buffer full, drop oldest data
                        buffer.clear();
                        break;
                    }
                }
                // Search for linebreak
                if let Some(pos) = buffer.iter().position(|&c| c == b'\n' || c == b'\r') {
                    // Copy up to linebreak into new Vec
                    let mut line = Vec::<u8, 128>::new();
                    for &b in &buffer[..pos] {
                        let _ = line.push(b);
                    }
                    // Move remaining bytes to start of buffer
                    let rest = buffer.len() - (pos + 1);
                    for i in 0..rest {
                        buffer[i] = buffer[pos + 1 + i];
                    }
                    buffer.truncate(rest);
                    return line;
                }
            }
            _ => {
                // No data read, yield to other tasks
                Timer::after_millis(1).await;
            }
        }
    }
}