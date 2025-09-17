use crate::db::record::{Band, Network, Record};
use crate::config::Config;

pub fn nmea_to_records(nmea_sentences: Vec<String>, config: &Config) -> Vec<Record> {
    let mut records = Vec::with_capacity(nmea_sentences.len() * 2); // NOTE: rough optimization
    let mut current_gps_time = i64::MAX;

    for sentence in nmea_sentences {
        // println!("Processing NMEA sentence: {}", sentence);
        if let Some(t) = find_gga_time(&sentence) {
            current_gps_time = t;
        }

        find_gsv_records_into(sentence, current_gps_time, config, &mut records);
    }

    records
}

fn command_to_network(header: &str) -> Option<Network> {
    if header.len() < 3 {
        return None;
    }
    let network = match &header[1..3] {
        "GP" => Network::GPS,
        "GA" => Network::Galileo,
        "GB" => Network::BeiDou,
        "GL" => Network::GLONASS,
        _ => Network::Unknown,
    };
    Some(network)
}

fn correct_sattelite_id(satellite: u32, network: Network) -> u32 {
    match network {
        Network::GLONASS => satellite - 64,
        Network::GPS => if satellite > 192 { satellite - 128 } else { satellite },
        _ => satellite,
    }
}

fn number_to_band(num: u32) -> Band {
    match num {
        1 => Band::L1,
        5 => Band::L5,
        7 => Band::L5,
        8 => Band::L5,
        _ => Band::Unknown,
    }
}

fn is_valid_nmea_sentence(sentence: &str) -> bool {
    sentence.starts_with('$') && sentence.contains('*')
}

fn is_nmea_command(header: &str, command: &str) -> bool {
    header.len() >= 6 && &header[3..6] == command
}

fn find_gga_time(sentence: &str) -> Option<i64> {
    let cleaned = sentence.trim();

    if !is_valid_nmea_sentence(cleaned) {
        return None;
    }

    let mut it = cleaned.split(|c| c == ',' || c == '*');

    let header = it.next()?;
    if !is_nmea_command(header, "GGA") {
        return None;
    }

    let time_str = it.next()?;

    if time_str.len() < 6 {
        return None;
    }

    let hours = time_str[0..2].parse::<i64>().ok()?;
    let minutes = time_str[2..4].parse::<i64>().ok()?;
    let seconds = time_str[4..6].parse::<i64>().ok()?;

    Some(hours * 3600 + minutes * 60 + seconds)
}

fn find_gsv_records_into(sentence: String, current_gps_time: i64, config: &Config, records: &mut Vec<Record>) {
    let cleaned = sentence.trim();
    if !is_valid_nmea_sentence(cleaned) {
        return;
    }

    let mut it = cleaned.split(|c| c == ',' || c == '*');

    let header = match it.next() {
        Some(h) if is_nmea_command(h, "GSV") => h,
        _ => return,
    };

    let network = match command_to_network(header) {
        Some(network) => network,
        None => return,
    };

    // Add band from signal id, as part of id

    let num_messages_str = match it.next() { Some(v) => v, _ => return };
    let num_messages = match num_messages_str.parse::<usize>() {
        Ok(v) => v,
        Err(_) => return,
    };

    let idx_of_message_str = match it.next() { Some(v) => v, _ => return };
    let idx_of_message = match idx_of_message_str.parse::<usize>() {
        Ok(v) => v,
        Err(_) => return,
    };

    let num_satellites_str = match it.next() { Some(v) => v, _ => return };
    let num_satellites = match num_satellites_str.parse::<usize>() {
        Ok(v) => v,
        Err(_) => return,
    };

    let num_records = if idx_of_message == num_messages {
        num_satellites - (num_messages - 1) * 4
    } else {
        4
    };

    let mut new_records = Vec::with_capacity(4);

    for _ in 0..num_records {
        let satellite_str = match it.next() { Some(v) if !v.is_empty() => v, _ => break };

        let satellite = match satellite_str.parse::<u32>() {
            Ok(v) => v,
            Err(_) => {
                it.next(); it.next(); it.next();
                continue;
            }
        };

        let elev_str = match it.next() { Some(v) => v, _ => break };
        let elevation = match elev_str.parse::<f64>() {
            Ok(v) => v,
            Err(_) => {
                it.next(); it.next();
                continue;
            }
        };

        let az_str = match it.next() { Some(v) => v, _ => break };
        let azimuth = match az_str.parse::<f64>() {
            Ok(v) => v,
            Err(_) => {
                it.next();
                continue;
            }
        };

        let snr_str = match it.next() { Some(v) => v, _ => break };
        let snr = match snr_str.parse::<f64>() {
            Ok(v) => v,
            Err(_) => continue,
        };

        if elevation < config.min_elevation || elevation > config.max_elevation {
            continue;
        }

        if azimuth < config.min_azimuth || azimuth > config.max_azimuth {
            continue;
        }

        new_records.push(Record {
            id: 0,
            satellite: correct_sattelite_id(satellite, network),
            elevation,
            azimuth,
            snr,
            time: current_gps_time,
            network,
            band: Band::Unknown,
        });
    }

    let band_str = match it.next() { Some(v) if !v.is_empty() => v, _ => return };
    let band_num = match band_str.parse::<u32>() {
        Ok(v) => v,
        Err(_) => return,
    };
    let band = number_to_band(band_num);

    for record in new_records.iter_mut() {
        record.band = band;
        record.id = (record.network as u32 + 1) * 10000 + (record.band as u32) * 1000 + record.satellite;
        records.push(record.clone());
    }
}