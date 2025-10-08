#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use core::time::Duration;
use embassy_executor::Spawner;
use embassy_time::Timer;
use esp_hal::{
    delay::Delay,
    gpio,
    gpio::{Input, InputConfig, Pull},
    main,
    rtc_cntl::{
        Rtc,
        SocResetReason,
        reset_reason,
        sleep::{TimerWakeupSource, WakeupLevel},
        wakeup_cause,
    },
    system::{Cpu},
    timer::systimer::SystemTimer,
    clock::CpuClock,
};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // generator version: 0.5.0

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    let mut rtc = Rtc::new(peripherals.LPWR);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    // TODO: Spawn some tasks
    let _ = spawner;

    let timer = TimerWakeupSource::new(Duration::from_secs(10));
    rtc.sleep_deep(&[&timer]);

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}
