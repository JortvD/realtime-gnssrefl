use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_futures::select::{select, Either};
use defmt::info;
use embassy_time::{Duration, Instant, Timer};
use crate::{battery::Battery, messages::{MonReqMsg, MonResMsg}};


#[embassy_executor::task]
pub async fn task_monitor(
    channel_req: &'static Channel<CriticalSectionRawMutex, MonReqMsg, 8>,
    channel_res: &'static Channel<CriticalSectionRawMutex, MonResMsg, 8>,
    mut battery: Battery,
) {
    let mut battery_mv: f32 = 0.0;
    let mut chip_temp_c: f32 = 0.0;
    let mut next_adc_measurement = Instant::now();

    info!("[moni] starting");
    
    loop {
        let mut adc_timer = Timer::at(next_adc_measurement);

        let result = select(
            &mut adc_timer,
            channel_req.receive()
        ).await;
        match result {
            Either::First(_) => {
                let start = Instant::now();
                match battery.get_battery_voltage().await {
                    Ok(volts) => {
                        battery_mv = calc_emwa(volts as f32, battery_mv, 0.02, 0.0, 4200.0);
                        info!("[moni] Battery millivolts {} (emwa {}) in {} ms", volts, battery_mv as u32, (Instant::now() - start).as_millis());
                        channel_res.send(MonResMsg::BatVoltSuccess { voltage: battery_mv as u32 }).await;
                    }
                    Err(_) => {
                        channel_res.send(MonResMsg::BatVoltFail).await;
                    }
                }

                let start = Instant::now();
                match battery.get_chip_temperature().await {
                    Ok(temp) => {
                        chip_temp_c = calc_emwa(temp, chip_temp_c, 0.02, -50.0, 85.0);
                        info!("[moni] Chip temperature: {} (emwa {}) in {} ms", temp, chip_temp_c, (Instant::now() - start).as_millis());
                        channel_res.send(MonResMsg::TempSuccess { temp_c: chip_temp_c }).await;
                    }
                    Err(_) => {
                        channel_res.send(MonResMsg::TempFail).await;
                    }
                }

                next_adc_measurement = next_adc_measurement.saturating_add(Duration::from_secs(30));
            }
            Either::Second(message) => {
                match message {
                    MonReqMsg::GetBatVolt => {
                        channel_res.send(MonResMsg::BatVoltSuccess { voltage: battery_mv as u32 }).await;
                    }
                    MonReqMsg::GetTemp => {
                        channel_res.send(MonResMsg::TempSuccess { temp_c: chip_temp_c }).await;
                    }
                }
            }
        }
    }
}

fn calc_emwa(now: f32, previous: f32, k: f32, min: f32, max: f32) -> f32 {
    if now <= min {
        previous
    } else if now >= max {
        previous
    } else if previous == 0.0 {
        now
    } else {
        now * k + previous * (1.0-k)
    }
}