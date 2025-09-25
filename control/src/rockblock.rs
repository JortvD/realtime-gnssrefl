use defmt::info;

pub struct RockBlock;

impl RockBlock {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn send_data(&mut self, data: &[u8]) -> Result<(), ()> {
        info!("SENDING STUB: {:?}", data);
        Ok(())
    }
}