#[derive(Debug, Clone)]
pub struct Arc {
    pub sat_id: u32,
    pub time_start: i64,
    pub time_end: i64,
    pub record_indices: Vec<usize>,
}

impl Arc {
    pub fn new(sat_id: u32, time_start: i64, time_end: i64, record_indices: Vec<usize>) -> Self {
        Arc {
            sat_id,
            time_start,
            time_end,
            record_indices,
        }
    }
}

pub struct ArcDatabase {
    pub arcs: Vec<Arc>,
}

impl ArcDatabase {
    pub fn new() -> Self {
        ArcDatabase {
            arcs: Vec::new(),
        }
    }

    pub fn insert(&mut self, arc: Arc) {
        self.arcs.push(arc);
    }

    pub fn len(&self) -> usize {
        self.arcs.len()
    }

    pub fn check_memory(&self) -> usize {
        std::mem::size_of_val(&self.arcs) + self.arcs.capacity() * std::mem::size_of::<Arc>()
    }
}