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
use embassy_rp::watchdog::Watchdog;
use embassy_sync::mutex::Mutex;
use embassy_time::Duration;
use embassy_time::Instant;
use embassy_time::Timer;
use gpio::{Level, Output, Input, Pull};
use embassy_rp::bind_interrupts;
use embassy_rp::uart::InterruptHandler as UARTInterruptHandler;
use embassy_rp::peripherals::UART0;
use embassy_rp::peripherals::UART1;
use embassy_sync::channel::Channel;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use static_cell::StaticCell;
use embassy_rp::adc;

use {defmt_rtt as _, panic_probe as _};

mod nmea;
mod math;
mod storage;
mod types;
mod measure;
mod compute;
mod control;
mod utils;
mod gnss;
mod comms;
mod rockblock;
mod realtime;
mod scheduler;
mod messages;
mod battery;
mod monitor;

use crate::battery::Battery;
use crate::comms::task_comms;
use crate::compute::task_compute;
use crate::control::task_control;
use crate::messages::MonResMsg;
use crate::messages::{MeasureReqMsg, ComputeReqMsg, CommReqMsg, MeasureResMsg, ComputeResMsg, CommResMsg};
use crate::gnss::GNSSSensor;
use crate::measure::task_measure;
use crate::monitor::task_monitor;
use crate::rockblock::RockBlock9704;
use crate::storage::FlashStorage;

static MEASURE_REQUEST_CHANNEL: Channel<CriticalSectionRawMutex, MeasureReqMsg, 8> = Channel::new();
static COMPUTE_REQUEST_CHANNEL: Channel<CriticalSectionRawMutex, ComputeReqMsg, 8> = Channel::new();
static COMM_REQUEST_CHANNEL: Channel<CriticalSectionRawMutex, CommReqMsg, 8> = Channel::new();
static MONITOR_REQUEST_CHANNEL: Channel<CriticalSectionRawMutex, MonReqMsg, 8> = Channel::new();
static MEASURE_RESPONSE_CHANNEL: Channel<CriticalSectionRawMutex, MeasureResMsg, 8> = Channel::new();
static COMPUTE_RESPONSE_CHANNEL: Channel<CriticalSectionRawMutex, ComputeResMsg, 8> = Channel::new();
static COMM_RESPONSE_CHANNEL: Channel<CriticalSectionRawMutex, CommResMsg, 8> = Channel::new();
static MONITOR_RESPONSE_CHANNEL: Channel<CriticalSectionRawMutex, MonResMsg, 8> = Channel::new();

pub const GNSS_UART_BAUDRATE: u32 = 115_200;
pub const ROCKBLOCK_UART_BAUDRATE: u32 = 230_400;

bind_interrupts!(pub struct Irqs {
    UART0_IRQ  => UARTInterruptHandler<UART0>;
    UART1_IRQ  => UARTInterruptHandler<UART1>;
    ADC_IRQ_FIFO => adc::InterruptHandler;
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

fn create_clock_config() -> ClockConfig {
    let result = ClockConfig::system_freq(15_000_000);

    if result.is_err() {
        error!("Failed to set system clock frequency");
    }

    result.unwrap()
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let start = Instant::now();
    info!("[main] startup");
    // Init peripherals
    let mut config: embassy_rp::config::Config = Default::default();
    config.clocks = create_clock_config();
    let p = embassy_rp::init(config);

    info!("[main] clock freq: {} MHz", clk_sys_freq() / 1_000_000);

    // Watchdog
    let mut wdg = Watchdog::new(p.WATCHDOG);
    wdg.pause_on_debug(false);
    wdg.start(Duration::from_secs(3));
    
    // Battery peripherals
    let mut pin_bat_stat1 = Input::new(p.PIN_16, Pull::None);
    let mut pin_bat_stat2 = Input::new(p.PIN_17, Pull::None);
    let mut pin_bat_CE = Output::new(p.PIN_18, Level::Low);

    let mut adc: adc::Adc<'_, adc::Async> = adc::Adc::new(p.ADC, Irqs, adc::Config::default());
    let mut pin_bat_voltage: adc::Channel<'_> = adc::Channel::new_pin(p.PIN_28, Pull::None);

    let battery = Battery::new(
        pin_bat_stat1, 
        pin_bat_stat2, 
        pin_bat_CE, 
        pin_bat_voltage, 
        adc);

    // LED peripherals
    let led: Output<'_> = Output::new(p.PIN_25, Level::Low);

    // GNSS peripherals
    let mut gnss_uart_config = uart::Config::default();
    gnss_uart_config.baudrate = GNSS_UART_BAUDRATE;
    let gnss_uart = uart::Uart::new(p.UART0, p.PIN_12, p.PIN_13, Irqs, p.DMA_CH0, p.DMA_CH1, gnss_uart_config);
    let mut gnss_sensor = GNSSSensor::new(gnss_uart);

    // Rockblock pheripherals
    let mut config_uart_rockblock = uart::Config::default();
    config_uart_rockblock.baudrate = ROCKBLOCK_UART_BAUDRATE;
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

    // Storage peripheral
    let storage = FlashStorage::new(p.FLASH, false);
    {
        *(STORAGE.lock().await) = Some(storage);
    }

    info!("[main] initialized peripherals");

    info!("[main] starting watchdog feeder task");
    static CELL: StaticCell<Watchdog> = StaticCell::new();
    let wdg: &'static mut Watchdog = CELL.init(wdg);

    let result = spawner.spawn(watchdog_feeder(wdg));
    if result.is_err() {
        error!("Failed to spawn watchdog feeder task: {}", result.unwrap_err());
    }

    info!("[main] spawning tasks");
    let result = spawner.spawn(task_measure(
        &MEASURE_REQUEST_CHANNEL,
        &MEASURE_RESPONSE_CHANNEL,
        &STORAGE, 
        gnss_sensor
    ));

    if result.is_err() {
        error!("Failed to spawn measure task: {}", result.unwrap_err());
    }
       
    let result = spawner.spawn(task_compute(
        &COMPUTE_REQUEST_CHANNEL, 
        &COMPUTE_RESPONSE_CHANNEL, 
        &STORAGE
    ));

    if result.is_err() {
        error!("Failed to spawn compute task: {}", result.unwrap_err());
    }

    let result = spawner.spawn(task_comms(
        &COMM_REQUEST_CHANNEL, 
        &COMM_RESPONSE_CHANNEL,
        &STORAGE,
        rockblock
    ));

    if result.is_err() {
        error!("Failed to spawn comms task: {}", result.unwrap_err());
    }

    let result = spawner.spawn(task_control(
        &MEASURE_REQUEST_CHANNEL,
        &COMPUTE_REQUEST_CHANNEL,
        &COMM_REQUEST_CHANNEL,
        &MEASURE_RESPONSE_CHANNEL,
        &COMPUTE_RESPONSE_CHANNEL,
        &COMM_RESPONSE_CHANNEL, 
    ));

    if result.is_err() {
        error!("Failed to spawn control task: {}", result.unwrap_err());
    }

    let result = spawner.spawn(led_blink(led));

    if result.is_err() {
        error!("Failed to spawn LED blink task: {}", result.unwrap_err());
    }

    let result = spawner.spawn(task_monitor(
        &MONITOR_REQUEST_CHANNEL, 
        &MONITOR_RESPONSE_CHANNEL, 
        battery)
    );

    if result.is_err() {
        error!("Failed to spawn Monitor task: {}", result.unwrap_err());
    }
    
    info!("[main] startup complete in {} ms", start.elapsed().as_millis());
}


#[embassy_executor::task]
async fn led_blink(mut led: Output<'static>) {
    info!("[led_]: starting");
    loop {
        led.set_high();
        Timer::after_millis(25).await;
        led.set_low();
        Timer::after_millis(25).await;
    }
}

#[embassy_executor::task]
async fn watchdog_feeder(wdg: &'static mut Watchdog) {
    info!("[wdg_]: starting");
    loop {
        Timer::after(Duration::from_millis(500)).await;
        wdg.feed();
    }
}