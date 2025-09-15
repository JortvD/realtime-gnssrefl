use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct Record {
    pub id: u32,
    pub elevation: f64,
    pub azimuth: f64,
    pub snr: f64,
    pub time: i64,
}

pub struct RecordDatabase {
    pub records: VecDeque<Record>,
}

impl RecordDatabase {
    pub fn new() -> Self {
        RecordDatabase {
            records: VecDeque::new(),
        }
    }

    pub fn insert_many(&mut self, records: Vec<Record>) {
        self.records.extend(records);
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn check_memory(&self) -> usize {
        std::mem::size_of_val(&self.records) + self.records.capacity() * std::mem::size_of::<Record>()
    }
}