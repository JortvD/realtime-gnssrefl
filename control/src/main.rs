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
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::mutex::Mutex;
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
use embassy_sync::channel::Channel;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
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
mod utils;
mod gnss;
mod comms;
mod rockblock;
mod realtime;
mod scheduler;

use crate::comms::task_comms;
use crate::compute::task_compute;
use crate::control::task_control;
use crate::control::{MeasureReqMsg, ComputeReqMsg, CommReqMsg, MeasureResMsg, ComputeResMsg, CommResMsg};
use crate::gnss::GNSSSensor;
use crate::measure::task_measure;
use crate::rockblock::RockBlock;
use crate::storage::FlashStorage;
use crate::types::*;


static mut CORE1_STACK: Stack<4096> = Stack::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();
static MEASURE_REQUEST_CHANNEL: Channel<CriticalSectionRawMutex, MeasureReqMsg, 8> = Channel::new();
static COMPUTE_REQUEST_CHANNEL: Channel<CriticalSectionRawMutex, ComputeReqMsg, 8> = Channel::new();
static COMM_REQUEST_CHANNEL: Channel<CriticalSectionRawMutex, CommReqMsg, 8> = Channel::new();
static MEASURE_RESPONSE_CHANNEL: Channel<CriticalSectionRawMutex, MeasureResMsg, 8> = Channel::new();
static COMPUTE_RESPONSE_CHANNEL: Channel<CriticalSectionRawMutex, ComputeResMsg, 8> = Channel::new();
static COMM_RESPONSE_CHANNEL: Channel<CriticalSectionRawMutex, CommResMsg, 8> = Channel::new();

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

type StorageType = Mutex<CriticalSectionRawMutex, Option<FlashStorage>>;
static STORAGE: StorageType = Mutex::new(None);

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("[main] startup");
    // Init peripherals
    let mut config: embassy_rp::config::Config = Default::default();
    config.clocks = ClockConfig::system_freq(150_000_000).unwrap();
    let p = embassy_rp::init(config);

    let sys_freq = clk_sys_freq();
    info!("[main] using system clock: {} MHz", sys_freq / 1_000_000);
    
    // GPIOS
    let led: Output<'_> = Output::new(p.PIN_25, Level::Low);

    // UART GNSS
    let mut gnss_uart_config = uart::Config::default();
    gnss_uart_config.baudrate = 921_600;
    let gnss_uart = uart::Uart::new(p.UART0, p.PIN_12, p.PIN_13, Irqs, p.DMA_CH0, p.DMA_CH1, gnss_uart_config);
    let gnss_sensor = GNSSSensor::new(gnss_uart, types::Config::default());

    // UART Rockblock
    let mut config_uart_rockblock = uart::Config::default();
    config_uart_rockblock.baudrate = 921_600;
    let uart_rockblock = uart::Uart::new(p.UART1, p.PIN_8, p.PIN_9, Irqs, p.DMA_CH2, p.DMA_CH3, config_uart_rockblock);
    let rockblock = RockBlock::new(uart_rockblock);

    let storage = FlashStorage::new(p.FLASH, false);

    {
        *(STORAGE.lock().await) = Some(storage);
    }

    //let sector = Sector::new(0, 0, 45602, config.bins_per_sector, config.seconds_per_bin);

    info!("[main] initialized peripherals");

    // Spawn core 0 tasks
    spawner.spawn(task_measure(
        &MEASURE_REQUEST_CHANNEL,
        &MEASURE_RESPONSE_CHANNEL,
        &STORAGE, 
        gnss_sensor
    )).unwrap();
       
    spawner.spawn(task_compute(
        &COMPUTE_REQUEST_CHANNEL, 
        &COMPUTE_RESPONSE_CHANNEL, 
        &STORAGE
    )).unwrap();

    spawner.spawn(task_comms(
        &COMM_REQUEST_CHANNEL, 
        &COMM_RESPONSE_CHANNEL,
        &STORAGE,
        rockblock
    )).unwrap();

    spawner.spawn(task_control(
        &MEASURE_REQUEST_CHANNEL,
        &COMPUTE_REQUEST_CHANNEL,
        &COMM_REQUEST_CHANNEL,
        &MEASURE_RESPONSE_CHANNEL,
        &COMPUTE_RESPONSE_CHANNEL,
        &COMM_RESPONSE_CHANNEL, 
    )).unwrap();

    spawner.spawn(led_blink(led)).unwrap();
    
    info!("[main] spawned core 0 tasks");

    // Spawn core 1 tasks
    // spawn_core1(
    //     p.CORE1,
    //     unsafe { &mut *core::ptr::addr_of_mut!(CORE1_STACK) },
    //     move || {
    //         let executor1 = EXECUTOR1.init(Executor::new());
    //         executor1.run(|spawner| {
                
    //         });
    //     },
    // );

    // info!("Spawned core 1 tasks");
}


#[embassy_executor::task]
async fn led_blink(mut led: Output<'static>) {
    info!("[LED]: starting");
    loop {
        led.set_high();
        Timer::after_millis(25).await;
        led.set_low();
        Timer::after_millis(25).await;
    }
}

// #[embassy_executor::task]
// async fn task0() {
//     loop {
//         info!("Task0");
//         Timer::after_millis(1000).await;
//     }
// }

// #[embassy_executor::task]
// async fn task1() {
//     loop {
//         info!("Task1");
//         Timer::after_millis(1000).await;
//                 let x = 1;
//         let y = 0;
//         let _ = x / y; // division by zero triggers a crash
//         //defmt::panic!("Task1 panic");
//     }
// }





