use heapless::String;
use core::fmt::Write;

pub fn time_str_to_seconds(time_str: &str) -> Option<u32> {
    let parts: heapless::Vec<&str, 3> = time_str.split(':').collect::<heapless::Vec<_, 3>>();
    if parts.len() != 3 {
        return None;
    }

    let hours = parts[0].parse::<u32>().ok()?;
    let minutes = parts[1].parse::<u32>().ok()?;
    let seconds = parts[2].parse::<u32>().ok()?;

    Some(hours * 3600 + minutes * 60 + seconds)
}


pub fn seconds_to_time_str(seconds: u32) -> String<8> {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    let mut time_str = String::<8>::new();
    write!(time_str, "{:02}:{:02}:{:02}", hours, minutes, secs).ok();
    time_str
}

/// Days since the Unix epoch (1970-01-01). Works for all proleptic Gregorian dates.
#[inline]
pub fn days_from_civil(mut y: i32, m: u32, d: u32) -> i64 {
    // Shift March to month 0 to simplify leap-year math.
    let m_i32 = m as i32;
    y -= if m_i32 <= 2 { 1 } else { 0 };

    // Floor-divide by 400-year eras.
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = (y - era * 400) as i64;                  // [0, 399]
    let mp  = (m_i32 + if m_i32 > 2 { -3 } else { 9 }) as i64; // Mar=0 … Feb=11
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;       // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;   // [0, 146096]

    // 719_468 = days from 0000-03-01 to 1970-01-01
    era as i64 * 146_097 + doe - 719_468
}

/// Inverse of `days_from_civil`.
#[inline]
pub fn date_from_days(days: i64) -> (i32, u32, u32) {
    // Translate to days since 0000-03-01 to use the March-based arithmetic.
    let z   = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097;                                     // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let mut y = yoe + era * 400;                                     // year-of-era
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);               // [0, 365]
    let mp  = (5 * doy + 2) / 153;                                   // [0, 11]
    let d   = (doy - (153 * mp + 2) / 5 + 1) as u32;                 // [1, 31]
    let m_i64 = mp + if mp < 10 { 3 } else { -9 };                   // [1, 12]
    let m = m_i64 as u32;

    // Undo the March shift: Jan/Feb belong to the next civil year.
    y += if m <= 2 { 1 } else { 0 };

    (y as i32, m, d)
}


pub fn date_to_str(days: u32) -> String<11> {
    let (year, month, day) = date_from_days(days as i64);
    let mut date_str = String::<11>::new();
    write!(date_str, "{:02}-{:02}-{:04}", day, month, year).unwrap();
    date_str
}

pub fn parse_lat(lat_str: &str, northsouth_str: &str) -> Option<f32> {
    if lat_str.len() < 4 || northsouth_str.len() != 1 {
        return None;
    }

    let degrees = lat_str[0..2].parse::<f32>().ok()?;
    let minutes = lat_str[2..].parse::<f32>().ok()?;

    let mut lat = degrees + (minutes / 60.0);

    if northsouth_str == "S" {
        lat = -lat;
    } else if northsouth_str != "N" {
        return None;
    }

    Some(lat)
}

pub fn parse_lon(lon_str: &str, eastwest_str: &str) -> Option<f32> {
    if lon_str.len() < 5 || eastwest_str.len() != 1 {
        return None;
    }

    let degrees = lon_str[0..3].parse::<f32>().ok()?;
    let minutes = lon_str[3..].parse::<f32>().ok()?;

    let mut lon = degrees + (minutes / 60.0);

    if eastwest_str == "W" {
        lon = -lon;
    } else if eastwest_str != "E" {
        return None;
    }

    Some(lon)
}