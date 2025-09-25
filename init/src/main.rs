use anyhow::Result;
use serialport::{DataBits, FlowControl, Parity, StopBits};
use std::io::Write;
use std::time::Duration;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::thread::sleep;

fn main() -> Result<()> {
    let port_path = "/dev/ttyACM0";
    let baud = 921_600;

    let builder = serialport::new(port_path, baud)
        .data_bits(DataBits::Eight)
        .parity(Parity::None)
        .stop_bits(StopBits::One)
        .flow_control(FlowControl::None)
        .timeout(Duration::from_millis(1000));

    let mut port = builder.open()?; // open the serial device

    // Read all lines from the log file
    let file = File::open("data/nmea.txt")?;
    let lines: Vec<String> = BufReader::new(file)
        .lines()
        .collect::<Result<_, _>>()?;

    // Split the log into parts, each part ends at a line starting with "$GNRMC" (not included)
    let mut parts = Vec::new();
    let mut current_part = Vec::new();
    for line in lines {
        if line.starts_with("$GNRMC") {
            if !current_part.is_empty() {
                parts.push(std::mem::take(&mut current_part));
            }
        }
        current_part.push(line);
    }
    if !current_part.is_empty() {
        parts.push(current_part);
    }

    // Write each part to the serial port, one part per second
    for (i, part) in parts.iter().enumerate() {
        for line in part {
            writeln!(port, "{}", line)?;
        }
        println!("Wrote {} lines to {}", part.len(), port_path);
        port.flush()?;
        
        if i % 240 == 0 || i % 240 == 1 {
            sleep(Duration::from_millis(800));
        }
        else {
            sleep(Duration::from_millis(40));
        }
    }

    Ok(())
}
