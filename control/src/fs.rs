use littlefs2::{
    driver::Storage,
};

struct FileStorage {}

impl Storage for FileStorage {
    type CACHE_SIZE = U512;
    type LOOKAHEAD_SIZE: ArrayLength<u64>;

    const READ_SIZE: usize;
    const WRITE_SIZE: usize;
    const BLOCK_SIZE: usize;
    const BLOCK_COUNT: usize;
    const BLOCK_CYCLES: isize = -1isize;

    // Required methods
    fn read(&mut self, off: usize, buf: &mut [u8]) -> Result<usize> {
        
    }

    fn write(&mut self, off: usize, data: &[u8]) -> Result<usize> {

    }

    fn erase(&mut self, off: usize, len: usize) -> Result<usize> {

    }
}