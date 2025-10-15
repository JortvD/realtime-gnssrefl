use defmt::info;
use embassy_rp::{gpio, uart};
use embassy_time::{Instant, Timer};
use heapless::Vec;

use crate::{nmea::NmeaBurst, types::Nmealine, GNSS_POST_UART_BAUDRATE, GNSS_PRE_UART_BAUDRATE};

pub struct GNSSSensor {
    uart: uart::Uart<'static, uart::Async>,
    pin_power: gpio::Output<'static>,
    awake: bool,
}

pub struct UBXCommand {
    pub cls: u8,
    pub id: u8,
    pub length: u16,
    pub payload: [u8; 256],
}

pub struct CfgUpdateItem {
    pub key: u32,
    pub size: u8,
    pub value: u32,
}

impl CfgUpdateItem {
    pub fn new(key: u32, size: u8, value: u32) -> Self {
        Self { key, size, value }
    }

    pub fn uart1_baudrate(baudrate: u32) -> Self {
        Self {
            key: 0x40520001,
            size: 4,
            value: baudrate,
        }
    }
}

pub struct CfgUpdateLayers {
    pub ram_layer: bool,
    pub bbr_layer: bool,
    pub flash_layer: bool,
}

impl CfgUpdateLayers {
    pub fn default() -> Self {
        Self {
            ram_layer: true,
            bbr_layer: false,
            flash_layer: false,
        }
    }
}

pub struct CfgUpdate {
    pub layers: CfgUpdateLayers,
    pub items: Vec<CfgUpdateItem, 128>,
}

fn checksum(command: &UBXCommand) -> u16 {
    let mut ck_a: u8 = 0;
    let mut ck_b: u8 = 0;

    ck_a = ck_a.wrapping_add(command.cls);
    ck_b = ck_b.wrapping_add(ck_a);

    ck_a = ck_a.wrapping_add(command.id);
    ck_b = ck_b.wrapping_add(ck_a);

    ck_a = ck_a.wrapping_add((command.length & 0xFF) as u8);
    ck_b = ck_b.wrapping_add(ck_a);

    ck_a = ck_a.wrapping_add((command.length >> 8) as u8);
    ck_b = ck_b.wrapping_add(ck_a);

    for i in 0..command.length as usize {
        ck_a = ck_a.wrapping_add(command.payload[i]);
        ck_b = ck_b.wrapping_add(ck_a);
    }

    ((ck_b as u16) << 8) | (ck_a as u16)
}

impl GNSSSensor {
    pub fn new(uart: uart::Uart<'static, uart::Async>, pin_power: gpio::Output<'static>) -> Self {
        Self { uart, pin_power, awake: false }
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

                line.push(read_byte[0] as char).ok();

                match read_byte[0] {
                    b'\n' => {
                        break;
                    }

                    _ => {}
                }
            }   

            if !error_in_line {
                // info!("{}", line.as_str());
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

    pub async fn sleep(&mut self) {
        if !self.awake {
            info!("[gnss] GNSS already asleep");
            return;
        }

        self.pin_power.set_low();

        Timer::after_secs(5).await;

        info!("[gnss] GNSS put to sleep");
        self.awake = false;
    }

    pub async fn wake(&mut self) {
        if self.awake {
            info!("[gnss] GNSS already awake");
            return;
        }

        self.pin_power.set_high();

        Timer::after_secs(3).await;

        self.uart.set_baudrate(GNSS_PRE_UART_BAUDRATE);

        Timer::after_secs(1).await;

        let uart_baudrate_item = CfgUpdateItem::uart1_baudrate(115200);
        let items = [uart_baudrate_item];
        let update = CfgUpdate {
            layers: CfgUpdateLayers::default(),
            items: Vec::from(items),
        };
        let cmd = self.cfg_valset(update);
        self.send_ubx(cmd).await;
        
        Timer::after_secs(1).await;

        self.uart.set_baudrate(GNSS_POST_UART_BAUDRATE);

        info!("[gnss] GNSS woke up");
        self.awake = true;
    }

    pub fn cfg_valset(
        &self,
        value: CfgUpdate,
    ) -> UBXCommand {
        let mut payload = [0u8; 256];
        payload[1] = (value.layers.ram_layer as u8)
            | ((value.layers.bbr_layer as u8) << 1)
            | ((value.layers.flash_layer as u8) << 2);

        let mut offset = 4;
        for item in value.items {
            payload[offset..offset + 4].copy_from_slice(&item.key.to_le_bytes());
            payload[offset + 4..offset + 4 + item.size as usize].copy_from_slice(&item.value.to_le_bytes());
            offset += 4 + item.size as usize;
        }

        UBXCommand {
            cls: 0x06,
            id: 0x8A,
            length: offset as u16,
            payload,
        }
    }

    pub fn rxm_pmreq(
        &self,
        duration: u32,
        backup: bool,
        force: bool,
        uartrx: bool,
        extint0: bool,
        extint1: bool,
        spics: bool,
    ) -> UBXCommand {
        let mut payload = [0u8; 256];
        payload[4..8].copy_from_slice(&duration.to_le_bytes());
        payload[8] = (backup as u8) | ((force as u8) << 1);
        payload[12] = (uartrx as u8)
            | ((extint0 as u8) << 1)
            | ((extint1 as u8) << 2)
            | ((spics as u8) << 3);
        UBXCommand {
            cls: 0x02,
            id: 0x41,
            length: 16,
            payload,
        }
    }

    pub async fn send_ubx(&mut self, command: UBXCommand) {
        let mut packet: [u8; 263] = [0; 263];
        packet[0] = 0xB5; // Sync char 1
        packet[1] = 0x62; // Sync char 2
        packet[2] = command.cls;
        packet[3] = command.id;
        packet[4] = (command.length & 0xFF) as u8;
        packet[5] = (command.length >> 8) as u8;
        for i in 0..command.length as usize {
            packet[6 + i] = command.payload[i];
        }
        let checksum = checksum(&command);
        packet[6 + command.length as usize] = (checksum & 0xFF) as u8;
        packet[7 + command.length as usize] = (checksum >> 8) as u8;
        info!("[meas] Sending UBX command {:?}", &packet[..8 + command.length as usize]);
        match self.uart.write(&packet[..8 + command.length as usize]).await {
            Ok(_) => {}
            Err(e) => {
                info!("[meas] Error sending UBX command: {}", e);
            }
        }
    }
}