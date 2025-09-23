//! This example test the RP Pico on board LED.
//!
//! It does not work with the RP Pico W board. See `blinky_wifi.rs`.

#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::clocks::clk_sys_freq;
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

use crate::storage::FlashStorage;
use crate::nmea::parse_burst;
use crate::types::{Line, Burst};
use crate::nmea::BURST_SAT_SIZE;

// bind_interrupts!(pub struct Irqs {
//     UART0_IRQ  => UARTInterruptHandler<UART0>;
//     TRNG_IRQ => embassy_rp::trng::InterruptHandler<TRNG>;
// });


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
    let p = embassy_rp::init(Default::default());

    let sys_freq = clk_sys_freq();
    info!("System clock frequency: {} MHz", sys_freq / 1_000_000);
    
    // GPIOS
    let mut led: Output<'_> = Output::new(p.PIN_25, Level::Low);

    // UART PI
    // let config_uart_pi = uart::Config::default();
    // let uart_pi: uart::Uart<'_, uart::Blocking> = uart::Uart::new_blocking(p.UART1, p.PIN_4, p.PIN_5, config_uart_pi);
   
    // UART GPS
    let config_uart_gps = uart::Config::default();
    let uart_gps = uart::Uart::new(p.UART0, p.PIN_16, p.PIN_17, Irqs, p.DMA_CH0, p.DMA_CH1, config_uart_gps);

    info!("Starting storage test");

    let mut storage = FlashStorage::new(p.FLASH, true);

    info!("Generating data");

    let mut data = [0u8; 4096 * 15];
    for i in 0..data.len() {
        data[i] = b'a' + 26 - (i % 26) as u8;
    }

    // info!("Data: {:?}", &data[..]);

    info!("Erasing");

    storage.erase(0).expect("erase failed");

    info!("Writing");

    storage.write(0, &data).expect("write failed");

    info!("Reading");

    let mut data2 = [0u8; 4096 * 15];

    storage.read(0, &mut data2).expect("read failed");

    // info!("Data: {:?}", &data2[..]);

    // Spawn tasks
    spawner.spawn(uart_heartbeat(uart_gps)).unwrap();
    spawner.spawn(led_blink(led)).unwrap();
}

#[embassy_executor::task]
async fn uart_heartbeat(mut uart_gps: uart::Uart<'static, uart::Async>) {
    let mut burst = Burst::new();
    loop {
        // Get burst
        loop {
            // Get line
            let mut line = Line::new(); 
            let mut error_in_line = false;
            loop {
                // Get byte

                // Check if read is succesful
                let mut read_byte: [u8; 1] = [0; 1];
                match uart_gps.read(&mut read_byte).await {
                    Ok(_) => {

                    },
                    Err(e) => {
                        error_in_line = true;
                        continue;   
                    },
                }     
                
                // Add byte as char to line
                line.push(read_byte[0] as char).unwrap();

                // Stop iterating this line if byte is end of line
                match read_byte[0] {
                    b'\n' => {
                        break;
                    }

                    _ => {}
                }
            }   

            // Add line to burst if it was read without errors
            if !error_in_line {
                burst.push(line).unwrap();
            }

            // Stop iterating this burst if line is $GNGLL line
            if let Some(last) = burst.last() {
                if last.len() >= 6 {
                    match &last.as_str()[..6] {
                        "$GNGLL" => {
                            break;
                        }
                        _ => {}
                    }
                }
            }

        }

        // A full burst has been collected, do something with it
        let parsed_burst = nmea::parse_burst(&burst);
        
        for line in parsed_burst {
            info!("0x{:x}", line);
        }

        burst.clear();
    }
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

fn transform_u32_to_array_of_u8(x:u32) -> [u8;4] {
    let b1 : u8 = ((x >> 24) & 0xff) as u8;
    let b2 : u8 = ((x >> 16) & 0xff) as u8;
    let b3 : u8 = ((x >> 8) & 0xff) as u8;
    let b4 : u8 = (x & 0xff) as u8;
    return [b1, b2, b3, b4]
}