use rppal::uart::{Parity, Uart};
use std::time::Duration;

fn main() {
    let mut uart = Uart::with_path("/dev/ttyAMA0", 115_200, Parity::None, 8, 1).expect("Failed to open UART");
    uart.set_read_mode(1, Duration::from_millis(0)).expect("Failed to set read mode");

    loop {
        let mut buffer = [0u8; 1024];
        
        match uart.read(&mut buffer) {
            Ok(bytes_read) if bytes_read > 0 => {
                let data = &buffer[..bytes_read];
                if let Ok(text) = std::str::from_utf8(data) {
                    print!("{}", text);
                } else {
                    println!("{:?}", data);
                }
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("UART read error: {:?}", e);
            }
        }
    }
}
