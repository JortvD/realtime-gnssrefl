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
    let mut battery_mv: u32 = 0;
    let mut chip_temp_c: f32 = 0.0;
    let mut next_adc_measurement = Instant::now();

    info!("[moni] starting");
    
    loop {
        let mut adc_timer = Timer::at(next_adc_measurement);

        info!("[moni] waiting for request...");
        let result = select(
            &mut adc_timer,
            channel_req.receive()
        ).await;
        match result {
            Either::First(_) => {
                // info!("timer went off!");
                match battery.get_battery_voltage().await {
                    Ok(volts) => {
                        battery_mv = volts;
                        info!("[moni] Battery millivolts: {}", battery_mv);
                    }
                    Err(e) => {
                        info!("[moni] Reading ADC error: {}", e)
                    }
                }

                // match battery.get_chip_temperature().await {
                //     Ok(temp) => {
                //         chip_temp_c = temp;
                //         info!("[moni] Chip temperature: {}", chip_temp_c);
                //     }
                //     Err(e) => {
                //         info!("[moni] Chip temperature fail: {}", e)
                //     }
                // }
                // info!("Move timer");
                next_adc_measurement = next_adc_measurement.saturating_add(Duration::from_secs(10));
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