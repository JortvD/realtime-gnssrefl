//! This example test the RP Pico on board LED.
//!
//! It does not work with the RP Pico W board. See `blinky_wifi.rs`.

#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::clocks::ClockConfig;
use embassy_rp::gpio;
use embassy_rp::uart;
use embassy_sync::mutex::Mutex;
use embassy_time::Timer;
use gpio::{Level, Output};
use embassy_rp::bind_interrupts;
use embassy_rp::uart::InterruptHandler as UARTInterruptHandler;
use embassy_rp::peripherals::UART0;
use embassy_rp::peripherals::UART1;
use embassy_sync::channel::Channel;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

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

use crate::clock::clk_control;
use crate::comms::task_comms;
use crate::compute::task_compute;
use crate::control::task_control;
use crate::control::{MeasureReqMsg, ComputeReqMsg, CommReqMsg, MeasureResMsg, ComputeResMsg, CommResMsg};
use crate::gnss::GNSSSensor;
use crate::measure::task_measure;
use crate::rockblock::RockBlock9704;
use crate::storage::FlashStorage;

static MEASURE_REQUEST_CHANNEL: Channel<CriticalSectionRawMutex, MeasureReqMsg, 8> = Channel::new();
static COMPUTE_REQUEST_CHANNEL: Channel<CriticalSectionRawMutex, ComputeReqMsg, 8> = Channel::new();
static COMM_REQUEST_CHANNEL: Channel<CriticalSectionRawMutex, CommReqMsg, 8> = Channel::new();
static MEASURE_RESPONSE_CHANNEL: Channel<CriticalSectionRawMutex, MeasureResMsg, 8> = Channel::new();
static COMPUTE_RESPONSE_CHANNEL: Channel<CriticalSectionRawMutex, ComputeResMsg, 8> = Channel::new();
static COMM_RESPONSE_CHANNEL: Channel<CriticalSectionRawMutex, CommResMsg, 8> = Channel::new();

pub const UART_BAUDRATE: u32 = 115_200; //921_600

bind_interrupts!(pub struct Irqs {
    UART0_IRQ  => UARTInterruptHandler<UART0>;
    UART1_IRQ  => UARTInterruptHandler<UART1>;
});


// Program metadata for `picotool info`.
// This isn't needed, but it's recomended to have these minimal entries.
#[unsafe(link_section = ".bi_entries")]
#[used]
pub static PICOTOOL_ENTRIES: [embassy_rp::binary_info::EntryAddr; 4] = [
    embassy_rp::binary_info::rp_program_name!(c"GNSS_IR_Control"),
    embassy_rp::binary_info::rp_program_description!(
        c"This code for a PI Pico 2 controls the GNSS_IR sensor made by team Tahmo! "
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
    
    // GPIOS
    let led: Output<'_> = Output::new(p.PIN_25, Level::Low);

    // UART GNSS
    let mut gnss_uart_config = uart::Config::default();
    gnss_uart_config.baudrate = UART_BAUDRATE;
    let gnss_uart = uart::Uart::new(p.UART0, p.PIN_12, p.PIN_13, Irqs, p.DMA_CH0, p.DMA_CH1, gnss_uart_config);
    let gnss_sensor = GNSSSensor::new(gnss_uart, types::Config::default());

    // UART Rockblock
    let mut config_uart_rockblock = uart::Config::default();
    config_uart_rockblock.baudrate = 230_400;
    let uart_rockblock = uart::Uart::new(p.UART1, p.PIN_8, p.PIN_9, Irqs, p.DMA_CH2, p.DMA_CH3, config_uart_rockblock);
    let pin_power_enable = Output::new(p.PIN_10, Level::Low);
    let pin_iridium_enable = Output::new(p.PIN_26, Level::Low);
    let pin_iridium_status = gpio::Input::new(p.PIN_27, gpio::Pull::Up);
    let rockblock = RockBlock9704::new(
        uart_rockblock,
        pin_power_enable,
        pin_iridium_enable,
        pin_iridium_status
    );

    let storage = FlashStorage::new(p.FLASH, false);

    {
        *(STORAGE.lock().await) = Some(storage);
    }

    info!("[main] initialized peripherals");

    // // Spawn core 0 tasks
    // spawner.spawn(task_measure(
    //     &MEASURE_REQUEST_CHANNEL,
    //     &MEASURE_RESPONSE_CHANNEL,
    //     &STORAGE, 
    //     gnss_sensor
    // )).unwrap();
       
    // spawner.spawn(task_compute(
    //     &COMPUTE_REQUEST_CHANNEL, 
    //     &COMPUTE_RESPONSE_CHANNEL, 
    //     &STORAGE
    // )).unwrap();

    spawner.spawn(task_comms(
        &COMM_REQUEST_CHANNEL, 
        &COMM_RESPONSE_CHANNEL,
        &STORAGE,
        rockblock
    )).unwrap();

    // spawner.spawn(task_control(
    //     &MEASURE_REQUEST_CHANNEL,
    //     &COMPUTE_REQUEST_CHANNEL,
    //     &COMM_REQUEST_CHANNEL,
    //     &MEASURE_RESPONSE_CHANNEL,
    //     &COMPUTE_RESPONSE_CHANNEL,
    //     &COMM_RESPONSE_CHANNEL, 
    // )).unwrap();

    spawner.spawn(clk_control()).unwrap();

    spawner.spawn(led_blink(led)).unwrap();
    
    info!("[main] spawned core 0 tasks");
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




