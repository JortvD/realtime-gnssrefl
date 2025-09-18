use core::str::Split;

use heapless::{String, Vec};

const LINE_LENGTH: usize = 128;
const BURST_LINE_SIZE: usize = 64;
const BURST_SAT_SIZE: usize = 128;

const NETWORK_BITS: usize = 2;
const ELEVATION_BITS: usize = 7;
const AZIMUTH_BITS: usize = 9;
const SNR_BITS: usize = 6;
const BAND_BITS: usize = 1;

const NUM_BITS: usize = 8;

fn is_command(word: &str, command: &str) -> bool {
    word.starts_with('$') && word.len() == 6 && &word[4..6] == command
}

pub fn parse_burst(lines: Vec::<String<LINE_LENGTH>, BURST_LINE_SIZE>) -> Vec<u32, BURST_SAT_SIZE> {
    let mut current_gps_time = u32::MAX;
    let mut values = Vec::<u32, BURST_SAT_SIZE>::new();
    let mut num: u32 = 0;

    values.push(0).ok();

    for line in lines {
        // Split only by ',' to match the expected type for parse_gga
        let mut it = line.split(',');

        let command = match it.next() {
            Some(word) => word,
            None => "",
        };

        if is_command(command, "GGA") {
            current_gps_time = match parse_gga(it) {
                Some(t) => t,
                None => current_gps_time,
            }
        }
        else if is_command(command, "GSV") {
            parse_gsv(it, command, &mut values, &mut num);
        }
    }
    let mut header = current_gps_time;
    header <<= NUM_BITS;
    header += num;
    values[0] = header;

    values
}

fn parse_gga<'a>(mut it: Split<'a, char>) -> Option<u32> {
    let time_str = it.next()?;

    let hours = time_str[0..2].parse::<u32>().ok()?;
    let minutes = time_str[2..4].parse::<u32>().ok()?;
    let seconds = time_str[4..6].parse::<u32>().ok()?;

    Some(hours * 3600 + minutes * 60 + seconds)
}

fn command_to_network(command: &str) -> Option<u32> {
     if command.len() < 3 {
        return None;
    }
    return match &command[1..3] {
        "GP" => Some(0),
        "GA" => Some(1),
        "GB" => Some(2),
        "GL" => Some(3),
        _ => None,
    };
}

fn parse_word_as_u32(word: Option<&str>) -> Option<u32> {
    match word {
        Some(w) => w.parse::<u32>().ok(),
        None => None,
    }
}

fn fix_sat(sat: u32, network: u32) -> u32 {
    match network {
        3 => sat - 64,
        0 => if sat > 192 { sat - 128 } else { sat },
        _ => sat
    }
}

fn number_to_band(num: u32) -> u32 {
    match num {
        1 => 0,
        5 => 1,
        7 => 1,
        8 => 1,
        _ => 0,
    }
}

fn parse_gsv<'a>(mut it: Split<'a, char>, command: &str, values: &mut Vec::<u32, BURST_SAT_SIZE>, num: &mut u32) -> Option<()> {
    let network = match command_to_network(command) {
        Some(t) => t,
        None => return None
    };
    let num_msgs = parse_word_as_u32(it.next())?;
    let msg_idx = parse_word_as_u32(it.next())?;
    let num_sats = parse_word_as_u32(it.next())?;

    let num_recs = if msg_idx == num_msgs {
        num_sats - (num_msgs - 1) * 4
    } else { 4 };

    let mut new_values = Vec::<u32, 4>::new();

    for _ in 0..num_recs {
        let mut value: u32 = 0;

        let sat = match it.next()?.parse::<u32>() {
            Ok(v) => v,
            Err(_) => {
                it.next(); it.next(); it.next();
                continue;
            }
        };

        // 7 bits
        value += fix_sat(sat, network);

        value <<= NETWORK_BITS;
        value += network;
        
        let elev = match it.next()?.parse::<u32>() {
            Ok(v) => v,
            Err(_) => {
                it.next(); it.next();
                continue;
            }
        };

        value <<= ELEVATION_BITS;
        value += elev;

        let azim = match it.next()?.parse::<u32>() {
            Ok(v) => v,
            Err(_) => {
                it.next();
                continue;
            }
        };

        value <<= AZIMUTH_BITS;
        value += azim;

        let snr = match it.next()?.parse::<u32>() {
            Ok(v) => v,
            Err(_) => {
                continue;
            }
        };

        value <<= SNR_BITS;
        value += snr;

        new_values.push(value).ok();
    }

    let band = match it.next()?[..1].parse::<u32>() {
        Ok(v) => number_to_band(v),
        Err(_) => return None,
    };

    for value in new_values.iter_mut() {
        *value <<= BAND_BITS;
        *value += band;
        values.push(*value).ok();
        *num += 1;
    }

    None
}