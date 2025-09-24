use defmt::info;
use embassy_rp::uart;
use embassy_time::{Duration, Instant, Timer};
use heapless::Vec;

use crate::{nmea, storage::BinStorage, types::{Burst, NmeaBurst, Nmealine, Sector, BIN_BURST_SIZE, BURST_SAMPLE_SIZE}};

#[embassy_executor::task]
pub async fn run_measure(mut uart_gps: uart::Uart<'static, uart::Async>, mut nmea: nmea::NMEAParser, sector: Sector, mut storage: BinStorage) {
    let mut bin_data = Vec::<u8, {4 * BURST_SAMPLE_SIZE * BIN_BURST_SIZE}>::new();
    let mut last_bin_id: u32 = sector.start_bin_id;

    loop {
        let (nmeaburst, n_bytes, duration) = get_burst(&mut uart_gps).await;
        let start_time = Instant::now() - duration;
        info!("[measure][?????] received NMEA burst of {} bytes in {} ms", n_bytes, duration.as_millis());

        // A full burst has been collected, do something with it
        let (burst, time, num) = nmea.parse_burst(&nmeaburst);

        if sector.is_time_before_sector(time) {
            info!("[measure][{:05}] time is before current sector, skipping", time);
            sleep_until_next(start_time, sector.measure_interval).await;
            continue;
        }

        if time == u32::MAX {
            info!("[measure][?????] no valid GPS time in burst, skipping");
            sleep_until_next(start_time, sector.measure_interval).await;
            continue;
        }

        if sector.is_time_after_sector(time) {
            info!("[measure][{:05}] time is later than current sector, STOPPING", time);
            break;
        }

        info!("[measure][{:05}] parsed burst with {} samples", time, num);

        let bin_id = sector.get_bin_for_time(time);

        if bin_id != last_bin_id {
            write_bin_to_storage(last_bin_id, &bin_data, &mut storage);
            bin_data.clear();
            info!("[measure][{:05}] switched to new bin {}, wrote previous bin {} to storage", time, bin_id, last_bin_id);
            last_bin_id = bin_id;
        }

        let data = burst_to_bytes(&burst);

        bin_data.extend_from_slice(&data).expect("Bin data overflow");

        let elapsed = Instant::now() - start_time;
        info!("[measure][{:05}] burst added to bin {}, elapsed {} ms, bin size {} bytes", time, bin_id, elapsed.as_millis(), bin_data.len());
        sleep_until_next(start_time, sector.measure_interval).await;
    }

    write_bin_to_storage(last_bin_id, &bin_data, &mut storage);
    bin_data.clear();
    info!("[measure][?????] wrote last bin {} to storage", last_bin_id);
}

async fn sleep_until_next(start: Instant, duration_secs: u32) {
    let buffer = embassy_time::Duration::from_millis(50);
    
    let now = Instant::now();
    let target = start + embassy_time::Duration::from_secs(duration_secs as u64);
    if target > now + buffer {
        let sleep_time = target - now - buffer;
        info!("[measure][?????] sleeping for {} ms until next measurement", sleep_time.as_millis());
        Timer::after(sleep_time).await;
    }
}

fn write_bin_to_storage(bin_id: u32, bin_data: &[u8], storage: &mut BinStorage) {
    if bin_data.len() == 0 {
        info!("[measure][?????] no data to write for bin {}, skipping", bin_id);
        return;
    }

    storage.write(bin_id, bin_data).expect("Failed to write bin to storage");
}

fn burst_to_bytes(burst: &Burst) -> Vec<u8, {4 * BURST_SAMPLE_SIZE}> {
    let mut data = Vec::<u8, {4 * BURST_SAMPLE_SIZE}>::new();

    for sample in burst.iter() {
        let bytes = sample.to_le_bytes();
        data.extend_from_slice(&bytes).expect("Burst to bytes overflow");
    }

    data
}

async fn get_burst(uart_gps: &mut uart::Uart<'static, uart::Async>) -> (NmeaBurst, u32, Duration) {
    let mut nmeaburst = NmeaBurst::new();
    let mut n_bytes: u32 = 0;
    let mut start: Instant = Instant::now();

    // Get burst
    loop {
        // Get line
        let mut line = Nmealine::new(); 
        let mut error_in_line = false;
        loop {
            // Get byte

            // Check if read is succesful
            let mut read_byte: [u8; 1] = [0; 1];
            match uart_gps.read(&mut read_byte).await {
                Ok(_) => {
                    n_bytes += 1;
                },
                Err(_) => {
                    error_in_line = true;
                    continue;   
                },
            }

            if n_bytes == 1 {
                start = Instant::now();
            }
            
            // Add byte as char to line
            line.push(read_byte[0] as char).unwrap();

            // Stop iterating this line if byte is end of line
            match read_byte[0] {
                b'\n' => {
                    break;
                }

                _ => {}
            }
        }   

        // Add line to burst if it was read without errors
        if !error_in_line {
            nmeaburst.push(line).unwrap();
        }

        // Stop iterating this burst if line is $GNGLL line
        if let Some(last) = nmeaburst.last() {
            if last.len() >= 6 {
                match &last.as_str()[..6] {
                    "$GNGLL" => {
                        break;
                    }
                    _ => {}
                }
            }
        }

    }

    (nmeaburst, n_bytes, Instant::now() - start)
}