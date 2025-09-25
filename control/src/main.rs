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
use embassy_rp::peripherals::UART1;
use embassy_rp::multicore::Stack;
use static_cell::StaticCell;
use embassy_rp::multicore::spawn_core1;
use embassy_executor::Executor;
use heapless::Vec;

use {defmt_rtt as _, panic_probe as _};

mod nmea;
mod math;
mod storage;
mod types;
mod measure;
mod compute;
mod clock;
mod control;

use crate::compute::run_compute;
use crate::control::core0_task_control;
use crate::control::core1_task_control;
use crate::measure::run_measure;
use crate::storage::FlashStorage;
use crate::types::*;

static mut CORE1_STACK: Stack<4096> = Stack::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();

bind_interrupts!(pub struct Irqs {
    UART0_IRQ  => UARTInterruptHandler<UART0>;
    UART1_IRQ  => UARTInterruptHandler<UART1>;
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

    let mut config = types::Config::default();
    
    
    // GPIOS
    let mut led: Output<'_> = Output::new(p.PIN_25, Level::Low);

    // UART GPS
    let mut config_uart_gps = uart::Config::default();
    config_uart_gps.baudrate = 921_600;
    let uart_gps = uart::Uart::new(p.UART0, p.PIN_16, p.PIN_17, Irqs, p.DMA_CH0, p.DMA_CH1, config_uart_gps);

    // UART Rockblock
    let mut config_uart_rockblock = uart::Config::default();
    config_uart_rockblock.baudrate = 921_600;
    let uart_rockblock = uart::Uart::new(p.UART1, p.PIN_8, p.PIN_9, Irqs, p.DMA_CH2, p.DMA_CH3, config_uart_rockblock);

    let storage = FlashStorage::new(p.FLASH, false);
    let bin_storage = storage::BinStorage::new(50, 240, storage);

    let sector = Sector::new(0, 45602, 30, 1, 240, 50);

    // Spawn core 0 task
    spawner.spawn(core0_task_control(
        spawner,
        uart_gps, 
        uart_rockblock, 
        led, 
        bin_storage)).unwrap();

    // Spawn core 1 task
    spawn_core1(
        p.CORE1,
        unsafe { &mut *core::ptr::addr_of_mut!(CORE1_STACK) },
        move || {
            let executor1 = EXECUTOR1.init(Executor::new());
            executor1.run(|spawner| {
                spawner.spawn(core1_task_control(spawner)).unwrap();
            });
        },
    );
}


// #[embassy_executor::task]
// async fn led_blink(mut led: Output<'static>) {
//     loop {
//         led.set_high();
//         Timer::after_millis(25).await;
//         led.set_low();
//         Timer::after_millis(25).await;
//     }
// }