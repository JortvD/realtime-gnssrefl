use heapless::{Vec, String};

pub const MAX_LINES: usize = 100;
pub const LINE_LEN: usize = 256;

pub type Line = String<LINE_LEN>;
pub type Burst = Vec<Line, MAX_LINES>;