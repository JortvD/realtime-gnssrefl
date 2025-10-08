use rppal::uart::{Parity, Uart};
use std::time::{Duration, Instant};

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

    let mut time = 0;
    loop {
        let start_time = Instant::now();
        let nmea_lines = get_nmea(time);
        for line in nmea_lines {
            uart.write(line.as_bytes()).expect("Failed to write to UART");
            uart.write(b"\r\n").expect("Failed to write line ending");
        }
        time += 1;
        let elapsed = start_time.elapsed();
        println!("Sent NMEA for time {}, elapsed {} us", time - 1, elapsed.as_micros());
        std::thread::sleep(Duration::from_secs(1) - elapsed);
    }
}

fn time_to_hhmmss(time: u32) -> String {
    let hours = time / 3600;
    let minutes = (time % 3600) / 60;
    let seconds = time % 60;
    format!("{:02}{:02}{:02}", hours, minutes, seconds)
}

fn get_nmea(time: u32) -> Vec<String> {
    // Convert seconds of the day to HHMMSS
    let hhmmss = time_to_hhmmss(time);

    let lines = vec![
        format!("$GNRMC,{},A,5159.913996,N,00422.396968,E,0.25,242.04,160925,,,D,V*01", hhmmss),
        "$GNVTG,242.04,T,,M,0.25,N,0.47,K,D*22".to_string(),
        format!("$GNGGA,{},5159.913996,N,00422.396968,E,2,22,1.12,32.678,M,47.111,M,,*49", hhmmss),
        "$GNGSA,A,3,01,06,09,04,,,,,,,,,1.40,1.12,0.84,1*00".to_string(),
        "$GNGSA,A,3,66,81,88,87,,,,,,,,,1.40,1.12,0.84,2*0F".to_string(),
        "$GNGSA,A,3,15,08,,,,,,,,,,,1.40,1.12,0.84,3*04".to_string(),
        "$GNGSA,A,3,19,35,29,,,,,,,,,,1.40,1.12,0.84,4*0A".to_string(),
        "$GPGSV,4,1,13,03,66,071,,19,41,269,16,01,38,138,27,17,37,239,20,1*6C".to_string(),
        "$GPGSV,4,2,13,06,36,305,15,09,33,208,28,34,29,191,35,04,67,164,18,1*64".to_string(),
        "$GPGSV,4,3,13,31,27,068,,28,17,038,,02,10,141,13,12,04,334,,1*69".to_string(),
        "$GPGSV,4,4,13,25,02,001,,1*52".to_string(),
        "$GPGSV,1,1,04,01,38,138,27,04,67,164,27,06,36,305,18,09,33,208,28,8*6A".to_string(),
        "$GLGSV,2,1,07,66,27,231,26,72,40,045,,81,37,321,19,88,74,213,18,1*72".to_string(),
        "$GLGSV,2,2,07,87,17,157,28,73,14,026,,80,04,335,,1*43".to_string(),
        "$GAGSV,3,1,09,26,05,306,,15,66,140,21,03,66,098,,13,55,300,18,7*74".to_string(),
        "$GAGSV,3,2,09,08,41,192,29,05,23,047,,34,15,130,,23,07,321,10,7*76".to_string(),
        "$GAGSV,3,3,09,31,03,007,,7*4C".to_string(),
        "$GAGSV,1,1,02,08,41,192,24,15,66,140,19,1*7F".to_string(),
        "$GBGSV,4,1,15,48,,,17,19,58,238,14,35,52,279,16,29,38,186,24,1*47".to_string(),
        "$GBGSV,4,2,15,09,30,076,,06,29,061,,16,27,052,,39,26,045,,1*7F".to_string(),
        "$GBGSV,4,3,15,44,17,318,16,05,15,118,,32,14,061,,23,10,045,,1*75".to_string(),
        "$GBGSV,4,4,15,22,06,240,17,37,06,092,,20,69,065,,1*43".to_string(),
        "$GBGSV,1,1,04,19,58,238,18,35,52,279,20,29,38,186,23,48,,,20,5*4C".to_string(),
        format!("$GNGLL,5159.913996,N,00422.396968,E,{},A,D*42", hhmmss),
    ];
    lines
}