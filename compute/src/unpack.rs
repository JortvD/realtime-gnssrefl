use crate::types::{Band, Config, Network, Record};

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

pub fn unpack(data: Vec<u32>, config: &Config) -> Vec<Record> {
    let mut records = Vec::new();

    let mut it = data.iter();
    
    loop {
        let header = match it.next() {
            Some(&v) => v,
            None => break,
        };

        let time = (header >> 8).into();
        let num_recs = (header & 0xFF) as usize;

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
            let sat_id = ((packed >> offset) & MASK1) as u32;

            if elevation < config.min_elevation || elevation > config.max_elevation {
                continue;
            }

            if azimuth < config.min_azimuth || azimuth > config.max_azimuth {
                continue;
            }

            records.push(Record {
                id: 0,
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