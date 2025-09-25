use defmt::info;

use embassy_rp::uart;

pub struct RockBlock{
    uart: uart::Uart<'static, uart::Async>,
}

impl RockBlock {
    pub fn new(uart: uart::Uart<'static, uart::Async>) -> Self {
        Self {uart}
    }

    pub async fn send_data(&mut self, data: &[u8]) -> Result<(), ()> {
        info!("SENDING STUB: {:?}", data);
        Ok(())
    }
}