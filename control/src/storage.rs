use embassy_rp::{flash::{Blocking, Error, Flash}, peripherals::FLASH, Peri};
use embassy_time::Instant;
use defmt::*;

use crate::types::{Measurement, SectorList, BINS_CONTAINER_START, BLOCK_SIZE, CONTAINER_SIZE, FLASH_SIZE, MEASUREMENTS_CONTAINER_START, MEASUREMENT_SIZE, MEASUREMENT_STORAGE_SIZE, NUM_CONTAINERS, SECTOR_CONTAINER_START, SECTOR_LIST_SIZE, START_ADDRESS, USABLE_SIZE};

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
        self.get_storage_start() + container_id as u32 * CONTAINER_SIZE as u32
    }

    pub fn write(&mut self, container_id: usize, offset: u32, data: &[u8]) -> Result<(), Error> {
        if offset as usize + data.len() > CONTAINER_SIZE {
            error!("Data size exceeds container size");
            return Err(Error::Other);
        }

        let start_time = if self.timing { Some(Instant::now()) } else { None };
        let addr = self.get_container_address(container_id) + offset;

        if self.timing {
            info!("Writing {} bytes to flash at address {}", data.len(), addr);
        }

        self.flash.blocking_write(addr, &data)?;

        if let Some(start) = start_time {
            info!("Write took {} ms", (Instant::now() - start).as_millis());
        }

        Ok(())
    }

    pub fn read(&mut self, container_id: usize, offset: u32, buffer: &mut [u8]) -> Result<(), Error> {
        if offset as usize + buffer.len() > CONTAINER_SIZE {
            error!("Buffer size exceeds container size");
            return Err(Error::Other);
        }

        let start_time = if self.timing { Some(Instant::now()) } else { None };
        let addr = self.get_container_address(container_id) + offset;
        let buf_len = buffer.len();

        if self.timing {
            info!("Reading {} bytes from flash at address {}", buf_len, addr);
        }

        self.flash.blocking_read(addr, &mut buffer[..buf_len])?;

        if let Some(start) = start_time {
            info!("Read took {} ms", (Instant::now() - start).as_millis());
        }

        Ok(())
    }

    pub fn erase(&mut self, container_id: usize) -> Result<(), Error> {
        self.partial_erase(container_id, 0, CONTAINER_SIZE as u32)
    }

    pub fn partial_erase(&mut self, container_id: usize, start_offset: u32, end_offset: u32) -> Result<(), Error> {
        let start_time = if self.timing { Some(Instant::now()) } else { None };
        let start = self.get_container_address(container_id) + start_offset;
        let end = self.get_container_address(container_id) + end_offset;

        if self.timing {
            info!("Erasing flash from address {} to {}", start, end);
        }

        self.flash.blocking_erase(start, end)?;

        if let Some(start) = start_time {
            info!("Erase took {} ms", (Instant::now() - start).as_millis());
        }

        Ok(())
    }
}

pub struct BinStorage {}

impl BinStorage {
    pub fn new() -> Self {
        Self {}
    }

    pub fn get_container_id(&self, bin_id: u32) -> usize {
        BINS_CONTAINER_START + bin_id as usize
    }

    pub fn write(&self, storage: &mut FlashStorage, bin_id: u32, data: &[u8]) -> Result<(), Error> {
        storage.erase(self.get_container_id(bin_id))?;
        storage.write(self.get_container_id(bin_id), 0, data)
    }

    pub fn read(&self, storage: &mut FlashStorage, bin_id: u32, buffer: &mut [u8]) -> Result<(), Error> {
        storage.read(self.get_container_id(bin_id), 0, buffer)
    }
}

pub struct MeasurementStorage {}

impl MeasurementStorage {
    pub fn new() -> Self {
        Self {}
    }

    fn get_container_id(&self) -> usize {
        MEASUREMENTS_CONTAINER_START
    }

    pub fn store(&self, storage: &mut FlashStorage, location: u32, measurement: Measurement) -> Result<(), ()> {
        let data = measurement.to_bytes();
        let start_offset = location * MEASUREMENT_STORAGE_SIZE as u32;
        let end_offset = start_offset + MEASUREMENT_STORAGE_SIZE as u32;
        if end_offset as usize > CONTAINER_SIZE * BLOCK_SIZE {
            error!("Measurement location exceeds container size");
            return Err(());
        }
        // info!("Storing measurement at location {}, container {}, offset {}-{}", location, self.get_container_id(), start_offset, end_offset);
        storage.partial_erase(self.get_container_id(), start_offset, end_offset).map_err(|_| ())?;
        storage.write(self.get_container_id(), start_offset as u32, &data).map_err(|_| ())?;
        Ok(())
    }

    pub fn read(&self, storage: &mut FlashStorage, location: u32) -> Option<Measurement> {
        let offset = location * MEASUREMENT_STORAGE_SIZE as u32;
        if (offset + MEASUREMENT_STORAGE_SIZE as u32) as usize > CONTAINER_SIZE * BLOCK_SIZE {
            error!("Measurement location exceeds container size");
            return None;
        }
        // info!("Reading measurement at location {}, container {}, offset {}", location, self.get_container_id(), offset);
        let mut data = [0u8; MEASUREMENT_SIZE];
        storage.read(self.get_container_id(), offset, &mut data).ok()?;

        Some(Measurement::from_bytes(&data))
    }
}

pub struct SectorStorage {}

impl SectorStorage {
    pub fn new() -> Self {
        Self {}
    }

    pub fn load(&self, storage: &mut FlashStorage) -> Result<SectorList, Error> {
        let mut buffer = [0u8; SECTOR_LIST_SIZE];
        storage.read(SECTOR_CONTAINER_START, 0, &mut buffer)?;

        Ok(SectorList::from_bytes(&buffer).ok_or(Error::Other)?)
    }

    pub fn save(&self, storage: &mut FlashStorage, sectors: &SectorList) -> Result<(), Error> {
        storage.write(SECTOR_CONTAINER_START, 0, &sectors.to_bytes())?;
        Ok(())
    }
}