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

    pub fn write(&mut self, container_id: usize, offset: u32, data: &[u8]) -> Result<(), Error> {
        if offset as usize + data.len() > CONTAINER_SIZE * BLOCK_SIZE {
            error!("Data size exceeds container size");
            return Err(Error::Other);
        }

        let start_time = if self.timing { Some(Instant::now()) } else { None };
        let addr = self.get_container_address(container_id) + offset;

        self.flash.blocking_write(addr, &data)?;

        if let Some(start) = start_time {
            info!("Write took {} ms", (Instant::now() - start).as_millis());
        }

        Ok(())
    }

    pub fn read(&mut self, container_id: usize, offset: u32, buffer: &mut [u8]) -> Result<(), Error> {
        if offset as usize + buffer.len() > CONTAINER_SIZE * BLOCK_SIZE {
            error!("Buffer size exceeds container size");
            return Err(Error::Other);
        }

        let start_time = if self.timing { Some(Instant::now()) } else { None };
        let addr = self.get_container_address(container_id) + offset;
        let buf_len = buffer.len();

        self.flash.blocking_read(addr, &mut buffer[..buf_len])?;

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

pub struct BinStorage {
    pub n_bins: u32,
    pub bin_time_size: u32,
    storage: FlashStorage,
}

impl BinStorage {
    pub fn new(n_bins: u32, bin_time_size: u32, storage: FlashStorage) -> Self {
        Self {
            n_bins,
            bin_time_size,
            storage,
        }
    }

    pub fn write(&mut self, bin_id: u32, data: &[u8]) -> Result<(), Error> {
        self.storage.erase(bin_id as usize)?;
        self.storage.write(bin_id as usize, 0, data)
    }

    pub fn read(&mut self, bin_id: u32, buffer: &mut [u8]) -> Result<(), Error> {
        self.storage.read(bin_id as usize, 0, buffer)
    }
}