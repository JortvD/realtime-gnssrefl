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
        let raw = self.adc.read(&mut self.pin_voltage).await?;
        // Convert to mV (assuming 3.3V reference, 12-bit ADC)
        let voltage_mv = raw as f32* 3300.0 / 4096.0;
        // Compensate for 1:1 voltage divider
        Ok((voltage_mv * 2.0) as u32)
    }

}