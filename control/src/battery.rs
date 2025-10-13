//Stat1 = fault 
//Stat2 = charge

//HH - Charge completed
//HL - Charging
//LH - Rec. fault
//HL - Non rec. fault

use crate::gpio;
use crate::adc;

pub struct Battery {
    pin_stat1: gpio::Input<'static>,
    pin_stat2: gpio::Input<'static>,
    pin_CE: gpio::Output<'static>,
    pin_voltage: adc::Channel<'static>,
    adc: adc::Adc<'static, adc::Async>
}

impl Battery {
    pub fn new(pin_stat1: gpio::Input<'static>, 
            pin_stat2: gpio::Input<'static>, 
            pin_CE: gpio::Output<'static>, 
            pin_voltage: adc::Channel<'static>, 
            adc: adc::Adc<'static, adc::Async> ) -> Self {
        Battery { 
            pin_stat1, 
            pin_stat2, 
            pin_CE, 
            pin_voltage, 
            adc 
        }
    }

    pub async fn get_battery_voltage(&mut self) -> Result<u32, adc::Error> {
        const NUM_SAMPLES: usize = 100;
        const SAMPLE_DELAY_MS: u64 = 10; // 100 samples per second (100 * 10 ms = 1 s)

        let mut sum: u32 = 0;

        for _ in 0..NUM_SAMPLES {
            sum += self.adc.read(&mut self.pin_voltage).await? as u32;
            embassy_time::Timer::after_millis(SAMPLE_DELAY_MS).await;
        }

        // Compute average ADC reading
        let avg_raw = sum as f32 / NUM_SAMPLES as f32;

        // Convert to mV (assuming 3.3 V reference, 12-bit ADC)
        let voltage_mv = avg_raw * 3300.0 / 4096.0;

        // Compensate for 1:1 voltage divider
        Ok((voltage_mv * 2.0) as u32)
    }


}