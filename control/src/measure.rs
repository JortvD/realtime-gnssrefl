use defmt::info;
use embassy_rp::uart;
use embassy_time::{Duration, Instant, Timer};
use heapless::{String, Vec};
use core::{fmt::Write, time};

use crate::{nmea, storage::BinStorage, types::{Burst, NmeaBurst, Nmealine, Sector, BIN_BURST_SIZE, BURST_SAMPLE_SIZE}};

#[embassy_executor::task]
pub async fn run_measure(mut uart_gps: uart::Uart<'static, uart::Async>, mut nmea: nmea::NMEAParser, sector: Sector, mut storage: BinStorage) {
    let mut bin_data = Vec::<u8, {4 * BURST_SAMPLE_SIZE * BIN_BURST_SIZE}>::new();
    let mut last_bin_id: u32 = sector.start_bin_id;

    loop {
        let (nmeaburst, n_bytes, duration) = get_burst(&mut uart_gps).await;
        let start_time = Instant::now() - duration;
        info!("[measure][????????] received NMEA burst of {} bytes in {} ms", n_bytes, duration.as_millis());

        // A full burst has been collected, do something with it
        let (burst, time, num) = nmea.parse_burst(&nmeaburst);

        let time_str = seconds_to_time_str(time);

        if sector.is_time_before_sector(time) {
            info!("[measure][{}] time is before current sector, skipping", time_str.as_str());
            continue;
        }

        if time == u32::MAX {
            info!("[measure][{}] no valid GPS time in burst, skipping", time_str.as_str());
            continue;
        }

        if sector.is_time_after_sector(time) {
            info!("[measure][{}] time is later than current sector, STOPPING", time_str.as_str());
            break;
        }

        info!("[measure][{}] parsed burst with {} samples", time_str.as_str(), num);

        let bin_id = sector.get_bin_for_time(time);

        if bin_id != last_bin_id {
            write_bin_to_storage(last_bin_id, &bin_data, &mut storage);
            bin_data.clear();
            info!("[measure][{}] switched to new bin {}, wrote previous bin {} to storage", time_str.as_str(), bin_id, last_bin_id);
            last_bin_id = bin_id;
        }

        let data = burst_to_bytes(&burst);

        bin_data.extend_from_slice(&data).expect("Bin data overflow");

        let elapsed = Instant::now() - start_time;
        info!("[measure][{}] burst added to cached bin {}, elapsed {} ms, cache size {} bytes", time_str.as_str(), bin_id, elapsed.as_millis(), bin_data.len());
    }

    write_bin_to_storage(last_bin_id, &bin_data, &mut storage);
    bin_data.clear();
    info!("[measure][????????] wrote last bin {} to storage", last_bin_id);
}

fn seconds_to_time_str(seconds: u32) -> String<8> {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    let mut time_str = String::<8>::new();
    write!(time_str, "{:02}:{:02}:{:02}", hours, minutes, secs).unwrap();
    time_str
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
        let byte0 = (*sample & 0xFF) as u8;
        let byte1 = ((*sample >> 8) & 0xFF) as u8;
        let byte2 = ((*sample >> 16) & 0xFF) as u8;
        let byte3 = ((*sample >> 24) & 0xFF) as u8;
        data.push(byte0).unwrap();
        data.push(byte1).unwrap();
        data.push(byte2).unwrap();
        data.push(byte3).unwrap();
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

            // info!("Read byte: {}", read_byte[0] as char);

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