use embassy_rp::{flash::{Blocking, Error, Flash}, peripherals::FLASH, Peri};
use embassy_time::Instant;
use defmt::*;

const FLASH_SIZE: usize = 4 * 1024 * 1024; // 4MB
const NUM_CONTAINERS: usize = 51;
const BLOCK_SIZE: usize = 4096; // 4KB
const CONTAINER_SIZE: usize = 15;
const USABLE_SIZE: usize = NUM_CONTAINERS * CONTAINER_SIZE * BLOCK_SIZE; // ~3MB
const START_ADDRESS: u32 = 0x100000; // Start at 1MB offset (flash-relative)


pub struct FlashStorage {
    timing: bool,
    pub(crate) flash: Flash<'static, FLASH, Blocking, FLASH_SIZE>,
}

impl FlashStorage {
    pub fn new(flash_ref: Peri<'static, FLASH>, timing: bool) -> Self {
        let flash = Flash::new_blocking(flash_ref);

        if USABLE_SIZE / NUM_CONTAINERS % BLOCK_SIZE != 0 {
            error!("Number of containers must divide the usable flash size evenly");
        }

        if START_ADDRESS as usize + USABLE_SIZE > FLASH_SIZE {
            error!("Storage exceeds flash size");
        }

        Self { 
            flash,
            timing
        }
    }

    fn get_storage_start(&self) -> u32 {
        START_ADDRESS
    }

    fn get_container_address(&self, container_id: usize) -> u32 {
        self.get_storage_start() + container_id as u32 * CONTAINER_SIZE as u32 * BLOCK_SIZE as u32
    }

    pub fn write(&mut self, container_id: usize, data: &[u8]) -> Result<(), Error> {
        if data.len() > CONTAINER_SIZE * BLOCK_SIZE {
            error!("Data size exceeds container size");
            return Err(Error::Other);
        }

        let start_time = if self.timing { Some(Instant::now()) } else { None };
        let offset = self.get_container_address(container_id);

        self.flash.blocking_write(offset, &data)?;

        if let Some(start) = start_time {
            info!("Write took {} ms", (Instant::now() - start).as_millis());
        }

        Ok(())
    }

    pub fn read(&mut self, container_id: usize, buffer: &mut [u8]) -> Result<(), Error> {
        if buffer.len() > CONTAINER_SIZE * BLOCK_SIZE {
            error!("Buffer size exceeds container size");
            return Err(Error::Other);
        }

        let start_time = if self.timing { Some(Instant::now()) } else { None };
        let offset = self.get_container_address(container_id);
        let buf_len = buffer.len();

        self.flash.blocking_read(offset, &mut buffer[..buf_len])?;

        if let Some(start) = start_time {
            info!("Read took {} ms", (Instant::now() - start).as_millis());
        }

        Ok(())
    }

    pub fn erase(&mut self, container_id: usize) -> Result<(), Error> {
        let start_time = if self.timing { Some(Instant::now()) } else { None };
        let start = self.get_container_address(container_id);
        let end = start + (CONTAINER_SIZE * BLOCK_SIZE) as u32;

        self.flash.blocking_erase(start, end)?;

        if let Some(start) = start_time {
            info!("Erase took {} ms", (Instant::now() - start).as_millis());
        }

        Ok(())
    }
}
