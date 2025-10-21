use anyhow::Result;
use serialport::{DataBits, FlowControl, Parity, StopBits};
use std::io::Write;
use std::time::{Duration, Instant};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter};
use std::thread::sleep;
use chrono::{Utc, Timelike, Datelike};

fn nmea_with_checksum(body: &str) -> String {
    let cs = body.bytes().fold(0u8, |a, b| a ^ b);
    format!("${}*{:02X}", body, cs)
}

fn tweak_line(line: &str, t: chrono::DateTime<chrono::Utc>) -> String {
    if !line.starts_with('$') { return line.to_string(); }
    let body = line.trim_start_matches('$');
    let (core, _) = body.split_once('*').unwrap_or((body, ""));
    let mut f: Vec<String> = core.split(',').map(|s| s.to_string()).collect();
    if f.is_empty() { return line.to_string(); }
    let kind = f[0].chars().rev().take(3).collect::<String>().chars().rev().collect::<String>(); // "RMC"/"GGA"
    let hhmmss = format!("{:02}{:02}{:02}.{:03}", t.hour(), t.minute(), t.second(), t.timestamp_subsec_millis());
    let ddmmyy = format!("{:02}{:02}{:02}", t.day(), t.month(), (t.year()%100) as i32);
    if kind == "RMC" && f.len() > 9 { f[9] = ddmmyy; }     // update date
    if kind == "GGA" && f.len() > 1 { f[1] = hhmmss; }     // update time
    nmea_with_checksum(&f.join(","))
}

fn main() -> Result<()> {
    // ==== tweakable options ====
    let extend_factor = 1;
    let port_path = "/dev/ttyACM0";
    let baud = 115_200;
    let min_part_interval_ms: u64 = 1000;
    // Set offset in seconds from current UTC time, or parse a custom time string (e.g., "2024-06-01T12:34:56Z")
    let mut time = chrono::DateTime::parse_from_rfc3339("2025-10-20T23:56:00Z")?.with_timezone(&Utc);
    // ===========================

    let builder = serialport::new(port_path, baud)
        .data_bits(DataBits::Eight)
        .parity(Parity::None)
        .stop_bits(StopBits::One)
        .flow_control(FlowControl::None)
        .timeout(Duration::from_millis(1000));
    let mut writer = BufWriter::new(builder.open()?); // buffer for performance

    let file = File::open("data/nmea.txt")?;
    // return Ok(());
    let lines: Vec<String> = BufReader::new(file).lines().collect::<Result<_, _>>()?;

    // Split into parts (each part ends just before next $GNRMC)
    let mut parts = Vec::new();
    let mut cur = Vec::new();
    for l in lines {
        if l.starts_with("$GNRMC") || l.starts_with("$GPRMC") { if !cur.is_empty() { parts.push(std::mem::take(&mut cur)); } }
        cur.push(l);
    }
    if !cur.is_empty() { parts.push(cur); }

    for _ in 0..extend_factor {
        for part in &parts {
            let part_start = Instant::now();
            for line in part {
                // "Now" is base + elapsed since we started streaming
                let out = tweak_line(line, time);
                writeln!(writer, "{}", out)?;
            }
            writer.flush()?;
            let elapsed = part_start.elapsed();
            if elapsed < Duration::from_millis(min_part_interval_ms) {
                println!(
                    "[{}] Part sent in {} ms, sleeping {} ms",
                    time.format("%Y-%m-%d %H:%M:%S"),
                    elapsed.as_millis(),
                    (min_part_interval_ms as i128 - elapsed.as_millis() as i128)
                );
                sleep(Duration::from_millis(min_part_interval_ms) - elapsed);
            }
            // Advance the RMC date seamlessly if we loop; keeps dates moving forward
            time = time + chrono::Duration::seconds(1);
        }
    }
    Ok(())
}
