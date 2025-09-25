
use defmt::info;
use embassy_time::Instant;
use heapless::Vec;

use crate::{rockblock::RockBlock, storage::MeasurementStorage, types::{Measurement, NUM_MEASUREMENTS}, StorageType};

pub struct MeasurementPacket {
    relative_height_mean: u16,
    relative_height_std: u8,
    num_observations_used: u8,
    num_observations_seen: u8,
}

impl MeasurementPacket {
    pub fn new(relative_height_mean: u16, relative_height_std: u8, num_observations_used: u8, num_observations_seen: u8) -> Self {
        Self {
            relative_height_mean,
            relative_height_std,
            num_observations_used,
            num_observations_seen,
        }
    }

    pub fn to_bytes(&self) -> [u8; 5] {
        let mut data = [0u8; 5];
        data[0..2].copy_from_slice(&self.relative_height_mean.to_le_bytes());
        data[2] = self.relative_height_std;
        data[3] = self.num_observations_used;
        data[4] = self.num_observations_seen;
        data
    }
}

const MAX_MEASUREMENT_PACKETS: usize = 10;

pub struct Packet {
    id_status: u16, // 13 bits id, 3 bits status
    timestamp: u16,
    battery: u8,
    measurements: Vec<MeasurementPacket, MAX_MEASUREMENT_PACKETS>,
}

impl Packet {
    pub fn new(id: u16, status: u16, timestamp: u16, battery: u8) -> Self {
        Self {
            id_status: (id & 0x1FFF) | ((status & 0x7) << 13),
            timestamp,
            battery,
            measurements: Vec::new(),
        }
    }

    pub fn push(&mut self, measurement: MeasurementPacket) -> Result<(), ()> {
        self.measurements.push(measurement).map_err(|_| ())
    }

    pub fn to_bytes(&self) -> Vec<u8, {5 * MAX_MEASUREMENT_PACKETS + 5}> {
        let mut data = Vec::<u8, {5 * MAX_MEASUREMENT_PACKETS + 5}>::new();
        data.extend_from_slice(&self.id_status.to_le_bytes()).ok();
        data.push(self.timestamp as u8).ok();
        data.push((self.timestamp >> 8) as u8).ok();
        data.push(self.battery).ok();
        for measurement in self.measurements.iter() {
            let meas_bytes = measurement.to_bytes();
            data.extend_from_slice(&meas_bytes).ok();
        }
        data
    }
}

#[embassy_executor::task]
pub async fn run_comms(
    mut rockblock: RockBlock,
    storage: &'static StorageType,
    index: u32,
    num_measurements: u32,
) {
    let mut packet = Packet::new(
        1,
        0,
        1234, 
        128
    );

    let measurement_storage = MeasurementStorage::new();

    for i in 0..num_measurements {
        let measurement: Option<Measurement>;
        {
            let mut storage_lock = storage.lock().await;
            let storage = storage_lock.as_mut().expect("Storage not initialized");
            measurement = measurement_storage.read(storage, (index + i) % NUM_MEASUREMENTS as u32);
        }
        if measurement.is_none() {
            info!("[comms] No measurement found at location {}, skipping", (index + i) % NUM_MEASUREMENTS as u32);
            continue;
        }
        let measurement = measurement.unwrap();
        info!("[comms] Read measurement at location {} with {} observations, mean {}, std {}", (index + i) % NUM_MEASUREMENTS as u32, measurement.observations.len(), measurement.mean, measurement.std);
        let packet_measurement = MeasurementPacket::new(
            mean_f32_to_u16(measurement.mean, 20.0),
            std_f32_to_u8(measurement.std, 1.0),
            measurement.observations.len() as u8,
            measurement.num_seen as u8,
        );
        packet.push(packet_measurement).ok();
    }

    let data = packet.to_bytes();
    info!("[comms] Sending packet with {} measurements, total size {} bytes", packet.measurements.len(), data.len());
    
    let start = Instant::now();
    rockblock.send_data(&data).await.expect("Failed to send data");
    info!("[comms] Packet sent in {} ms", (Instant::now() - start).as_millis());
}

fn mean_f32_to_u16(value: f32, max: f32) -> u16 {
    if value < 0.0 {
        return 0;
    }
    if value > max {
        return u16::MAX;
    }
    let scaled = (value / max) * (65_535.0);
    scaled as u16
}

fn std_f32_to_u8(value: f32, max: f32) -> u8 {
    if value < 0.0 {
        return 0;
    }
    if value > max {
        return 255;
    }
    let scaled = (value / max) * 255.0;
    scaled as u8
}