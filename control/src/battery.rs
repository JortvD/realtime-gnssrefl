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

    pub async fn get_battery_voltage(self) -> Result<u16, adc::Error> {
        self.adc.read(self.pin_voltage).await;
    }
}