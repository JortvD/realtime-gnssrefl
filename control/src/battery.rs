//Stat1 = fault 
//Stat2 = charge

use defmt::info;
use defmt::Format;
use embassy_time::with_timeout;
use embassy_time::Duration;
use embassy_futures::select::select;
use embassy_time::Timer;

use crate::gpio;
use crate::adc;

pub struct Battery {
    pin_stat1: gpio::Input<'static>,
    pin_stat2: gpio::Input<'static>,
    pin_CE: gpio::Output<'static>,
    pin_voltage: adc::Channel<'static>,
    pin_temp: adc::Channel<'static>,
    adc: adc::Adc<'static, adc::Async>,
    chargestate: ChargeState,
}

impl Battery {
    pub fn new(pin_stat1: gpio::Input<'static>, 
            pin_stat2: gpio::Input<'static>, 
            pin_CE: gpio::Output<'static>, 
            pin_voltage: adc::Channel<'static>, 
            pin_temp: adc::Channel<'static>,
            adc: adc::Adc<'static, adc::Async>,
            ) -> Self {
        Battery { 
            pin_stat1, 
            pin_stat2, 
            pin_CE, 
            pin_voltage, 
            pin_temp,
            adc,
            chargestate: ChargeState::Unknown,
        }
    }

    pub async fn get_battery_voltage(&mut self) -> Result<u32, adc::Error> {
        const NUM_SAMPLES: usize = 5;
        const SAMPLE_DELAY_MS: u64 = 10;

        let mut sum: u32 = 0;
        let mut actual_num: usize = 0;

        for _ in 0..NUM_SAMPLES {
            match with_timeout(Duration::from_millis(100), self.adc.read(&mut self.pin_voltage)).await {
                Ok(res) => {
                    sum += res? as u32;
                    actual_num += 1;
                },
                Err(e) => {}
            };
            embassy_time::Timer::after_millis(SAMPLE_DELAY_MS).await;
        }
        
        if actual_num == 0 {
            return Err(adc::Error::ConversionFailed);
        }

        // Compute average ADC reading
        let avg_raw = sum as f32 / actual_num as f32;

        // Convert to mV (assuming 3.3 V reference, 12-bit ADC)
        let voltage_mv = avg_raw * 3300.0 / 4096.0;

        // Compensate for 1:1 voltage divider
        Ok((voltage_mv * 2.0) as u32)
    }

    pub async fn get_chip_temperature(&mut self) -> Result<f32, adc::Error> {
        const NUM_SAMPLES: usize = 5;
        const SAMPLE_DELAY_MS: u64 = 10;

        let mut sum: u32 = 0;
        let mut actual_num: usize = 0;

        for _ in 0..NUM_SAMPLES {
            match with_timeout(Duration::from_millis(100), self.adc.read(&mut self.pin_temp)).await {
                Ok(res) => {
                    sum += res? as u32;
                    actual_num += 1;
                },
                Err(e) => {}
            };
            embassy_time::Timer::after_millis(SAMPLE_DELAY_MS).await;
        }

        if actual_num == 0 {
            return Err(adc::Error::ConversionFailed);
        }

        // Compute average ADC reading
        let avg_raw = sum as f32 / actual_num as f32;

        // Convert to mV (assuming 3.3 V reference, 12-bit ADC)
        let temp_c = convert_to_celsius(avg_raw);

        Ok(temp_c)
    }

    pub async fn wait_state_change (&mut self) -> ChargeState {
        if self.get_state() == self.chargestate {
            let s1 = self.pin_stat1.wait_for_any_edge();
            let s2 = self.pin_stat2.wait_for_any_edge();

            select(s1,s2).await;
        }

        self.chargestate = self.get_state();
        self.chargestate
    }

    pub fn get_state (&mut self) -> ChargeState {
        //HH - Completed
        //HL - Charging
        //LH - Rec. fault
        //HL - Non rec. fault
        match (self.pin_stat1.is_high(), self.pin_stat2.is_high()) {
            (true, true) => ChargeState::Completed,
            (true, false) => ChargeState::Charging,
            (false, true) => ChargeState::RecoverableFault,
            (false, false) => ChargeState::NonRecoverableFault 
        }
    }

    pub async fn toggle_ce(&mut self) {
        self.pin_CE.set_high();
        Timer::after_secs(1).await;
        self.pin_CE.set_low();
    }

}

fn convert_to_celsius(raw_temp: f32) -> f32 {
    // According to chapter 12.4.6 Temperature Sensor in RP235x datasheet
    let temp = 27.0 - (raw_temp as f32 * 3.3 / 4096.0 - 0.706) / 0.001721;
    let sign = if temp < 0.0 { -1.0 } else { 1.0 };
    let rounded_temp_x10: i16 = ((temp * 10.0) + 0.5 * sign) as i16;
    (rounded_temp_x10 as f32) / 10.0
}

#[derive(Format, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChargeState {
    Unknown,
    Completed,
    Charging,
    RecoverableFault,
    NonRecoverableFault
}