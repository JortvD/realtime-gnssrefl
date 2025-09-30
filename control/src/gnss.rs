use embassy_rp::uart;
use embassy_time::Instant;

use crate::{nmea::{NMEAParser, NmeaBurst}, types::{Config, Nmealine}};

pub struct GNSSSensor {
    uart: uart::Uart<'static, uart::Async>,
    pub parser: NMEAParser,
}

impl GNSSSensor {
    pub fn new(uart: uart::Uart<'static, uart::Async>, config: Config) -> Self {
        Self { uart, parser: NMEAParser::new(&config) }
    }

    pub async fn read_burst(&mut self) -> NmeaBurst {
        let mut nmeaburst = NmeaBurst::new();
        let mut n_bytes: u32 = 0;
        let mut start: Instant = Instant::now();

        loop {
            let mut line = Nmealine::new(); 
            let mut error_in_line = false;

            loop {
                let mut read_byte: [u8; 1] = [0; 1];

                match self.uart.read(&mut read_byte).await {
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

                line.push(read_byte[0] as char).expect("line buffer overflow");

                match read_byte[0] {
                    b'\n' => {
                        break;
                    }

                    _ => {}
                }
            }   

            if !error_in_line {
                nmeaburst.push(line);
            }

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

        nmeaburst.bytes = n_bytes;
        nmeaburst.duration = Instant::now() - start;

        nmeaburst
    }
}