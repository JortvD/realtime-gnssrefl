use anyhow::Result;
use serialport::{DataBits, FlowControl, Parity, StopBits};
use std::io::Write;
use std::time::Duration;

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
pub enum Network {
    GPS,
    Galileo,
    BeiDou,
    GLONASS,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum Band {
    L1,
    L5,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct Record {
    pub satellite: u32,
    pub elevation: f64,
    pub azimuth: f64,
    pub snr: f64,
    pub time: i64,
    pub network: Network,
    pub band: Band,
}

fn number_to_network(num: u32) -> Network {
    match num {
        0 => Network::GPS,
        1 => Network::Galileo,
        2 => Network::BeiDou,
        3 => Network::GLONASS,
        _ => Network::Unknown,
    }
}

fn number_to_band(num: u32) -> Band {
    match num {
        0 => Band::L1,
        1 => Band::L5,
        _ => Band::Unknown,
    }
}

pub fn unpack(data: Vec<u32>) -> Vec<Record> {
    let mut records = Vec::new();

    let mut it = data.iter();
    
    loop {
        let header = match it.next() {
            Some(&v) => v,
            None => break,
        };

        let time = (header >> 8).into();
        let num_recs = (header & 0xFF) as usize;

        if time == 16777215 {
            continue;
        }

        for _ in 0..num_recs {
            let packed = match it.next() {
                Some(&v) => v,
                None => break,
            };

            // format ABCDEF
            // A - 7 bit satellite ID
            // B - 2 bit network
            // C - 7 bit elevation (0-90)
            // D - 9 bit azimuth (0-359)
            // E - 6 bit SNR (0-64)
            // F - 1 bit band (0=L1, 1=L5)

            const MASK1: u32 = 0x1;
            const MASK2: u32 = 0x3;
            const MASK6: u32 = 0x3F;
            const MASK7: u32 = 0x7F;
            const MASK9: u32 = 0x1FF;

            let mut offset = 0;
            let band = (packed & MASK1) as u32;
            offset += 1;
            let snr = ((packed >> offset) & MASK6) as f64;
            offset += 6;
            let azimuth = ((packed >> offset) & MASK9) as f64;
            offset += 9;
            let elevation = ((packed >> offset) & MASK7) as f64;
            offset += 7;
            let network = ((packed >> offset) & MASK2) as u32;
            offset += 2;
            let sat_id = ((packed >> offset) & MASK7) as u32;

            records.push(Record {
                satellite: sat_id,
                network: number_to_network(network),
                band: number_to_band(band),
                elevation,
                azimuth,
                snr,
                time,
            });
        }
    }

    records
}

fn main() -> Result<()> {
    let port_path = "/dev/ttyACM0";
    let baud = 921_600;

    let builder = serialport::new(port_path, baud)
        .data_bits(DataBits::Eight)
        .parity(Parity::None)
        .stop_bits(StopBits::One)
        .flow_control(FlowControl::None)
        .timeout(Duration::from_millis(10000));

    let mut port = builder.open()?; // open the serial device

    port.write_all(&[1])?;
    port.flush()?;

    // Read all available incoming data
    
    let mut data = Vec::new();
    let mut bytes_read = 0;
    loop {
        let mut buffer = [0u8; 4096];
        match port.read(&mut buffer) {
            Ok(n) if n > 0 => {
                println!("Received {} bytes", n);
                bytes_read += n;
                data.extend_from_slice(&buffer[..n]);
                
            }
            Ok(_) => continue,
            Err(e) => {
                println!("Error reading from serial port: {:?}", e);
                break;
            }
        }
    }

    let mut u32_buffer = Vec::with_capacity(bytes_read / 4);
    for chunk in data.chunks(4) {
        let mut arr = [0u8; 4];
        for (i, &b) in chunk.iter().enumerate() {
            arr[i] = b;
        }
        u32_buffer.push(u32::from_le_bytes(arr));
    }

    let mut records: Vec<Record> = unpack(u32_buffer);
    println!("Unpacked {} records", records.len());

    let mut wtr = csv::Writer::from_path("records.csv")?;
    for record in &records {
        wtr.serialize(record)?;
    }
    wtr.flush()?;
    println!("Wrote {} records to records.csv", records.len());

    // // Read all lines from the log file
    // let file = File::open("data/nmea.txt")?;
    // let lines: Vec<String> = BufReader::new(file)
    //     .lines()
    //     .collect::<Result<_, _>>()?;

    // // Split the log into parts, each part ends at a line starting with "$GNRMC" (not included)
    // let mut parts = Vec::new();
    // let mut current_part = Vec::new();
    // for line in lines {
    //     if line.starts_with("$GNRMC") {
    //         if !current_part.is_empty() {
    //             parts.push(std::mem::take(&mut current_part));
    //         }
    //     }
    //     current_part.push(line);
    // }
    // if !current_part.is_empty() {
    //     parts.push(current_part);
    // }

    // // Write each part to the serial port, one part per second
    // for (i, part) in parts.iter().enumerate() {
    //     for line in part {
    //         writeln!(port, "{}", line)?;
    //     }
    //     println!("Wrote {} lines to {}", part.len(), port_path);
    //     port.flush()?;
        
    //     if i % 240 == 0 || i % 240 == 1 {
    //         sleep(Duration::from_millis(800));
    //     }
    //     else {
    //         sleep(Duration::from_millis(40));
    //     }
    // }

    Ok(())
}
