use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};

use crate::{battery::Battery, messages::{MonReqMsg, MonResMsg}};


#[embassy_executor::task]
pub async fn task_monitor(
    channel_req: &'static Channel<CriticalSectionRawMutex, MonitorMsg, 8>,
    channel_res: &'static Channel<CriticalSectionRawMutex, MonitorMsg, 8>,
    mut battery: Battery,
) {
    info!("[moni] starting");
    loop {
        info!("[moni] waiting for request...");
        let message = channel_req.receive().await;
        match message {
            MonReqMsg::GetBatVolt => {
                match battery.get_battery_voltage().await {
                    Ok(volts) => {
                        channel_res.send(MonResMsg::BatVoltSuccess { voltage: volts })
                    }
                    Err(e) => {
                        channel_res.send(MonResMsg::BatVoltFail)
                    }
                }
            }
        }
    }
}