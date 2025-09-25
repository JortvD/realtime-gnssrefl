use heapless::String;
use core::fmt::Write;

pub fn seconds_to_time_str(seconds: u32) -> String<8> {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    let mut time_str = String::<8>::new();
    write!(time_str, "{:02}:{:02}:{:02}", hours, minutes, secs).unwrap();
    time_str
}