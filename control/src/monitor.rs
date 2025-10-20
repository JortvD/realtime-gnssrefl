use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_futures::select::{select3, Either3};
use defmt::info;
use embassy_time::{Duration, Instant, Timer};
use core::cmp::max;
use crate::{battery::{Battery, ChargeState}, messages::{MonReqMsg, MonResMsg}};


#[embassy_executor::task]
pub async fn task_monitor(
    channel_req: &'static Channel<CriticalSectionRawMutex, MonReqMsg, 8>,
    channel_res: &'static Channel<CriticalSectionRawMutex, MonResMsg, 8>,
    mut battery: Battery,
) {
    let mut battery_mv: f32 = 0.0;
    let mut chip_temp_c: f32 = 0.0;
    let mut statefraction: u8 = 0;
    let mut next_adc_measurement = Instant::now();
    let mut charge_state_monitor = ChargeStateMonitor::new();

    info!("[moni] starting");
    
    loop {
        let mut adc_timer = Timer::at(next_adc_measurement);

        let result = select3(
            &mut adc_timer,
            channel_req.receive(),
            battery.wait_state_change()
        ).await;
        match result {
            Either3::First(_) => {
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
            Either3::Second(message) => {
                match message {
                    MonReqMsg::GetBatVolt => {
                        channel_res.send(MonResMsg::BatVoltSuccess { voltage: battery_mv as u32 }).await;
                    }
                    MonReqMsg::GetTemp => {
                        channel_res.send(MonResMsg::TempSuccess { temp_c: chip_temp_c }).await;
                    }
                    MonReqMsg::ResetChargeStateMonitor => {
                        charge_state_monitor.reset();
                    }

                }
            }
            Either3::Third(charge_state) => {
                info!("[moni] Charge controller state: {:?}", charge_state);
                charge_state_monitor.set_state(charge_state);

                statefraction = charge_state_monitor.get_fraction();
                channel_res.send(MonResMsg::ChargeStateFraction { fraction: charge_state_monitor.get_fraction() }).await;

                let (a,b,c,d) = unpack_fractions(statefraction);
                info!(
                    "[moni] ChargeState fraction byte: 0x{:02X}, completed: {:?}, charging: {:?}, recoverable: {:?}, nonrecoverable: {:?}",
                    statefraction, a, b, c, d
                );

                if charge_state == ChargeState::NonRecoverableFault {
                    info!("[moni] Toggling charge controller CE");
                    battery.toggle_ce().await;
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

struct ChargeStateMonitor {
    time_completed: u64,
    time_charging: u64,
    time_recoverable: u64,
    time_nonrecoverable: u64,

    last_time: Instant,
    current_state: ChargeState,
}

impl ChargeStateMonitor {
    pub fn new () -> Self{
        Self {
            time_completed: 0,
            time_charging: 0,
            time_recoverable: 0,
            time_nonrecoverable: 0,
            last_time: Instant::now(),
            current_state: ChargeState::Unknown
        }
    }

    pub fn reset(&mut self) {
        self.time_completed = 0;
        self.time_charging = 0;
        self.time_recoverable = 0;
        self.time_nonrecoverable = 0;

        self.last_time = Instant::now();
        self.current_state = ChargeState::Unknown;
    }

    pub fn set_state(&mut self, charge_state: ChargeState){
        // Every state is minimal 1 second
        let passed_time = (Instant::now() - self.last_time).min(Duration::from_secs(1)); 

        match self.current_state {
            ChargeState::Charging => self.time_charging += passed_time.as_secs(),
            ChargeState::Completed => self.time_completed += passed_time.as_secs(),
            ChargeState::RecoverableFault => self.time_recoverable += passed_time.as_secs(),
            ChargeState::NonRecoverableFault => self.time_nonrecoverable += passed_time.as_secs(),
            ChargeState::Unknown => {},
        }

        self.current_state = charge_state;
        self.last_time = Instant::now();
    }

    pub fn get_fraction(&mut self) -> u8 {
        self.set_state(self.current_state);

        let max_time = max(
            max(self.time_completed, self.time_charging),
            max(self.time_recoverable, self.time_nonrecoverable),
        ) as f32;

        if max_time == 0.0 {
            return 0;
        }

        let a = self.time_completed as f32 / max_time;
        let b = self.time_charging as f32 / max_time;
        let c = self.time_recoverable as f32 / max_time;
        let d = self.time_nonrecoverable as f32 / max_time;

        let qa = non_std_ceil(a * 3.0).min(3.0) as u8;
        let qb = non_std_ceil(b * 3.0).min(3.0) as u8;
        let qc = non_std_ceil(c * 3.0).min(3.0) as u8;
        let qd = non_std_ceil(d * 3.0).min(3.0) as u8;

        // Pack into 8 bits: [a:2][b:2][c:2][d:2]
        (qa << 6) | (qb << 4) | (qc << 2) | qd
    }

}

fn non_std_ceil(x: f32) -> f32 {
    let truncated = x as i32 as f32;

    if x == truncated {
        x
    } else if x > 0.0 {
        truncated + 1.0
    } else {
        truncated
    }
}

fn unpack_fractions(byte: u8) -> (u8, u8, u8, u8) {
    let a = (byte >> 6) & 0b11;
    let b = (byte >> 4) & 0b11;
    let c = (byte >> 2) & 0b11;
    let d = byte & 0b11;
    (a, b, c, d)
}