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
use embassy_rp::bind_interrupts;
use embassy_rp::uart::InterruptHandler as UARTInterruptHandler;
use embassy_rp::peripherals::UART0;
use heapless::{Vec, String};

use {defmt_rtt as _, panic_probe as _};

const MAX_LINES: usize = 100;
const LINE_LEN: usize = 256;

type Line = String<LINE_LEN>;
type Burst = Vec<Line, MAX_LINES>;

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
    // Init peripherals
    let p = embassy_rp::init(Default::default());

    // GPIOS
    let led: Output<'_> = Output::new(p.PIN_25, Level::Low);

    // UART PI
    let config_uart_pi = uart::Config::default();
    let uart_pi: uart::Uart<'_, uart::Blocking> = uart::Uart::new_blocking(p.UART1, p.PIN_4, p.PIN_5, config_uart_pi);
    
    // UART GPS
    let config_uart_gps = uart::Config::default();
    let uart_gps = uart::Uart::new(p.UART0, p.PIN_16, p.PIN_17, Irqs, p.DMA_CH0, p.DMA_CH1, config_uart_gps);

    // Spawn tasks
    spawner.spawn(uart_heartbeat(uart_pi, uart_gps)).unwrap();
    spawner.spawn(led_blink(led)).unwrap();
    
}

#[embassy_executor::task]
async fn uart_heartbeat(mut uart_pi: uart::Uart<'static, uart::Blocking>, mut uart_gps: uart::Uart<'static, uart::Async>) {
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
                        uart_pi.blocking_write("ERROR!".as_bytes()).unwrap();
                        uart_pi.blocking_write(&(e as u32+65).to_le_bytes()).unwrap();
                        uart_pi.blocking_write("\r\n".as_bytes()).unwrap();  

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
        print_burst(&mut uart_pi, &burst);
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

fn print_burst(uart_pi: &mut uart::Uart<'static, uart::Blocking>, burst: &Burst) {
    uart_pi.blocking_write("!!!START OF BURST!!!\r\n".as_bytes()).unwrap();
    for line in burst {
        uart_pi.blocking_write(line.as_bytes()).unwrap();
    }
    uart_pi.blocking_write("!!!END OF BURST!!!\r\n".as_bytes()).unwrap();
}