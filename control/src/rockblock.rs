use defmt::info;

use embassy_rp::uart;
use heapless::String;

pub enum RockBlockCommand {
    ERRR = 0,
    DMP1 = 1,
    DMP2 = 2,
}

pub struct RockBlock {
    uart: uart::Uart<'static, uart::Async>,
}

impl RockBlock {
    pub fn new(uart: uart::Uart<'static, uart::Async>) -> Self {
        Self {uart}
    }

    pub async fn send_data(&mut self, data: &[u8]) -> Result<(), ()> {
        info!("SENDING: {} bytes", data.len());
        self.uart.blocking_write(data).expect("Failed to write data");
        Ok(())
    }
    pub async fn receive(&mut self) -> Result<RockBlockCommand, ()> {
        let mut byte: [u8; 1] = [0; 1];

        return match self.uart.read(&mut byte).await {
            Ok(_) => {
                info!("Data received");
                match byte[0] {
                    1 => Ok(RockBlockCommand::DMP1),
                    2 => Ok(RockBlockCommand::DMP2),
                    _ => Ok(RockBlockCommand::ERRR),
                }
            },
            Err(_) => Ok(RockBlockCommand::ERRR)
        };
    }
}