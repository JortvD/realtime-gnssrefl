use rppal::uart::{Parity, Uart};
use std::time::Duration;

use crate::types::{Config, Record};

mod types;
mod unpack;
mod math;
mod gnssir;

impl Default for Config {
    fn default() -> Self {
        Config {
            min_elevation: 1.0,
            max_elevation: 10.0,
            min_azimuth: 0.0,
            max_azimuth: 360.0,
            min_height: 5.0,
            max_height: 30.0,
            step_size: 0.05,
        }
    }
}

fn main() {
    let mut uart = Uart::with_path("/dev/ttyAMA0", 115_200, Parity::None, 8, 1).expect("Failed to open UART");
    uart.set_read_mode(1, Duration::from_millis(100)).expect("Failed to set read mode");

    // Indicate ready to receive data
    uart.write("GO\r\n".as_bytes()).expect("Failed to write to UART");

    // Start receiving data
    let mut buffer = [0u8; 1024 * 1024 * 4];
    let mut bytes_read = 0;

    match uart.read(&mut buffer) {
        Ok(b) if b > 0 => {
            bytes_read += b;
            println!("Read {} bytes", b);
        }
        Ok(_) => {}
        Err(e) => {
            eprintln!("UART read error: {:?}", e);
        }
    }

    let mut u32_buffer = Vec::with_capacity(bytes_read / 4);
    for chunk in buffer[..bytes_read].chunks(4) {
        let mut arr = [0u8; 4];
        for (i, &b) in chunk.iter().enumerate() {
            arr[i] = b;
        }
        u32_buffer.push(u32::from_le_bytes(arr));
    }

    let original_chsum = (u32_buffer.pop().expect("No checksum found") as u64) << 32 | (u32_buffer.pop().expect("No checksum found") as u64);
    let chsum = fletcher::calc_fletcher64(&u32_buffer);

    if chsum != original_chsum {
        eprintln!("Checksum mismatch: calculated {:08X}, expected {:08X}", chsum, original_chsum);
        return;
    }

    let config = Config::default();
    let mut records: Vec<Record> = unpack::unpack(u32_buffer, &config);
    println!("Unpacked {} records", records.len());

    let arcs = gnssir::find_arcs(&records);
    let mut best_rhs = Vec::new();

    for arc in &arcs {
        println!("Arc ID {}: {} records from {} to {}", arc.sat_id, arc.record_indices.len(), arc.time_start, arc.time_end);
        gnssir::fix_arc_elev_azim(arc, &mut records);
        gnssir::correct_arc_snr(arc, &mut records);
        let rhs = gnssir::find_arc_rh(arc, &records, &config);
        let (rh, amp) = match gnssir::find_max_amplitude(&rhs) {
            Some((r, a)) => (r, a),
            None => {
                println!("  No frequency components found.");
                continue;
            }
        };
        println!("  Max relative height: {:.4} m with amplitude {:.4} volts/volts", rh, amp);
        best_rhs.push(rh);
    }

    // Remove outliers from best_rhs using IQR
    fn remove_outliers_iqr(data: &mut Vec<f64>) {
        if data.len() < 4 {
            return;
        }
        data.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let q1_idx = data.len() / 4;
        let q3_idx = 3 * data.len() / 4;
        let q1 = data[q1_idx];
        let q3 = data[q3_idx];
        let iqr = q3 - q1;
        let lower = q1 - 1.5 * iqr;
        let upper = q3 + 1.5 * iqr;
        data.retain(|&x| x >= lower && x <= upper);
    }

    let n_before = best_rhs.len();
    remove_outliers_iqr(&mut best_rhs);
    let n_after = best_rhs.len();
    println!("Removed {} outliers", n_before - n_after);

    let mean_rh = if !best_rhs.is_empty() {
        best_rhs.iter().sum::<f64>() / best_rhs.len() as f64
    } else {
        0.0
    };
    let stddev_rh = if best_rhs.len() > 1 {
        let mean = mean_rh;
        (best_rhs.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / (best_rhs.len() as f64 - 1.0)).sqrt()
    } else {
        0.0
    };

    uart.write(format!("{:2.3},{:2.3},{:3}\r\n", mean_rh, stddev_rh, n_after).as_bytes()).expect("Failed to write to UART");

}
