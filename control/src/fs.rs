use embassy_rp::{
    flash::{Async, Flash}, 
    peripherals::{DMA_CH4, FLASH}, 
    Peri
};

use embassy_util::Forever;

use littlefs2::{
    driver::Storage, fs::{Allocation, Filesystem}, io::Error
};
use littlefs2::consts::{U512, U1};

const FLASH_SIZE: usize = 2 * 1024 * 1024; // 2MB
const BLOCK_SIZE: usize = 4096; // 4KB
const BLOCK_COUNT: usize = FLASH_SIZE / BLOCK_SIZE;

pub struct FlashStorage {
    flash: Flash<'static, FLASH, Async, FLASH_SIZE>,
}

static FS_ALLOC: Forever<Allocation<FlashStorage>> = Forever::new();
static FS_STORAGE: Forever<FlashStorage> = Forever::new();

pub fn create_storage(
    flash_peri: Peri<'static, FLASH>,
    dma_ch: Peri<'static, DMA_CH4>,
) -> Filesystem<'static, FlashStorage> {
    let flash = Flash::<_, Async, FLASH_SIZE>::new(flash_peri, dma_ch);

    let storage: &'static mut FlashStorage = FS_STORAGE.put(FlashStorage { flash });
    let alloc: &'static mut Allocation<FlashStorage> = FS_ALLOC.put(Allocation::new());
    
    Filesystem::mount(alloc, storage).expect("Failed to mount filesystem")
}

impl Storage for FlashStorage {
    type CACHE_SIZE = U512;
    type LOOKAHEAD_SIZE = U1;

    const READ_SIZE: usize = 16;
    const WRITE_SIZE: usize = 512;
    const BLOCK_SIZE: usize = BLOCK_SIZE;
    const BLOCK_COUNT: usize = BLOCK_COUNT;
    const BLOCK_CYCLES: isize = -1;

    // Required methods
    fn read(&mut self, off: usize, buf: &mut [u8]) -> Result<usize, Error> {
        self.flash
            .blocking_read(off as u32, buf)
            .map_err(|_| Error::Io)?;
        Ok(buf.len())
    }

    fn write(&mut self, off: usize, data: &[u8]) -> Result<usize, Error> {
        self.flash
            .blocking_write(off as u32, data)
            .map_err(|_| Error::Io)?;
        Ok(data.len())
    }

    fn erase(&mut self, off: usize, len: usize) -> Result<usize, Error> {
        let from = off as u32;
        let to = (off + len) as u32;
        self.flash
            .blocking_erase(from, to)
            .map_err(|_| Error::Io)?;
        Ok(len)
    }
    
}