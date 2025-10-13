
use defmt::{info, Format};
use embassy_futures::select::{select, Either};
use embassy_time::{Instant, Timer};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use heapless::Vec;
use libm::floorf;

use crate::messages::{CommReqMsg, CommResMsg};
use crate::rockblock::{IMTMessage, RockBlock9704, BODY_SIZE, IMT_DEFAULT_TOPIC};
use crate::storage::MeasurementStorage;
use crate::types::{Measurement, Sector, MAX_SECTORS, NUM_MEASUREMENTS};
use crate::StorageType;

const MEASUREMENT_PACKET_SIZE: usize = 6; // bytes
const PACKET_HEADER_SIZE: usize = 4; // bytes

pub struct MeasurementPacket {
    uid: u8,
    relative_height_mean: u16,
    relative_height_std: u8,
    num_observations_used: u8,
    num_observations_seen: u8,
}

impl MeasurementPacket {
    pub fn new(uid: u8, relative_height_mean: u16, relative_height_std: u8, num_observations_used: u8, num_observations_seen: u8) -> Self {
        Self {
            uid,
            relative_height_mean,
            relative_height_std,
            num_observations_used,
            num_observations_seen,
        }
    }

    pub fn to_bytes(&self) -> [u8; MEASUREMENT_PACKET_SIZE] {
        let mut data = [0u8; MEASUREMENT_PACKET_SIZE];
        data[0] = self.uid;
        data[1..3].copy_from_slice(&self.relative_height_mean.to_le_bytes());
        data[3] = self.relative_height_std;
        data[4] = self.num_observations_used;
        data[5] = self.num_observations_seen;
        data
    }
}

const MAX_MEASUREMENT_PACKETS: usize = 10;

pub struct Packet { // 5 + n * 5 bytes = 55
    battery: u8,
    temp: u8,
    pub lat: u8,
    pub lon: u8,
    measurements: Vec<MeasurementPacket, MAX_MEASUREMENT_PACKETS>,
}

const PACKET_SIZE: usize = PACKET_HEADER_SIZE + (MAX_MEASUREMENT_PACKETS * MEASUREMENT_PACKET_SIZE);

impl Packet {
    pub fn new(battery: u8, temp: u8, lat: u8, lon: u8) -> Self {
        Self {
            battery,
            temp,
            lat,
            lon,
            measurements: Vec::new(),
        }
    }

    pub fn push(&mut self, measurement: MeasurementPacket) -> Result<(), ()> {
        self.measurements.push(measurement).map_err(|_| ())
    }

    pub fn to_bytes(&self) -> Vec<u8, PACKET_SIZE> {
        let mut data = Vec::<u8, PACKET_SIZE>::new();
        data.extend_from_slice(&self.battery.to_le_bytes()).ok();
        data.extend_from_slice(&self.temp.to_le_bytes()).ok();
        data.extend_from_slice(&self.lat.to_le_bytes()).ok();
        data.extend_from_slice(&self.lon.to_le_bytes()).ok();
        for measurement in self.measurements.iter() {
            let meas_bytes = measurement.to_bytes();
            data.extend_from_slice(&meas_bytes).ok();
        }
        data
    }
}

#[embassy_executor::task]
pub async fn task_comms(
    channel_req: &'static Channel<CriticalSectionRawMutex, CommReqMsg, 8>,
    channel_res: &'static Channel<CriticalSectionRawMutex, CommResMsg, 8>,
    storage: &'static StorageType,
    mut rockblock: RockBlock9704,
) {
    info!("[comm] starting");
    loop {
        info!("[comm] waiting for request...");
        let select = select(
            channel_req.receive(), 
            Timer::after_secs(u64::MAX)
        ).await;
        match select {
            Either::First(CommReqMsg::Send { sectors, config }) => {
                let mut uids: [u32; MAX_SECTORS] = [0; MAX_SECTORS];
                let mut count = 0;
                for s in sectors.iter() {
                    uids[count] = s.get_uid();
                    count += 1;
                }
                info!("[comm] starting communication for sectors {:?}", &uids[..count]);
                let result = run_comms(&mut rockblock, storage, sectors, config.num_send_measurements).await;
                if result.is_err() {
                    info!("[comm] communication failed");
                    channel_res.send(CommResMsg::Fail { 
                        sector_uids: heapless::Vec::from_slice(&uids[..count]).unwrap(),
                        error: result.unwrap_err() 
                    }).await;
                } else {
                    channel_res.send(CommResMsg::Success { 
                        sector_uids: heapless::Vec::from_slice(&uids[..count]).unwrap(),
                    }).await;
                }
            }
            Either::Second(_) => {}
        }
    }
}

#[derive(Debug, Format)]
pub enum CommsError {
    StorageAccess,
    RockBlockNoPower,
    RockBlockNotReady,
    RockBlockSendFail,
}

async fn run_comms(
    rockblock: &mut RockBlock9704,
    storage: &'static StorageType,
    sectors: Vec<Sector, MAX_SECTORS>,
    num_measurements: u32,
) -> Result<(), CommsError> {
    info!("[comm] Turning on RockBlock");
    rockblock.power_on().await;
    if rockblock.status != crate::rockblock::RockBlock9704Status::Unchecked {
        info!("[comm] RockBlock not ready to be checked, aborting comms");
        return Err(CommsError::RockBlockNoPower);
    }
    info!("[comm] Checking RockBlock status");
    rockblock.check_status().await;
    if rockblock.status != crate::rockblock::RockBlock9704Status::Ready {
        info!("[comm] RockBlock not ready, aborting comms");
        return Err(CommsError::RockBlockNotReady);
    }
    info!("[comm] RockBlock ready, preparing packet");

    let mut packet = Packet::new(
        1,
        0,
        0, 
        0
    );

    let mut highest_uid = u32::MIN;
    let mut highest_index = u32::MIN;
    let mut lat = 0f32;
    let mut lon = 0f32;

    for sector in sectors.iter() {
        if let Ok(measurement) = get_measurement_packet(storage, sector.get_measurement_index()).await {
            if sector.get_uid() > highest_uid {
                highest_uid = sector.get_uid();
                highest_index = sector.get_measurement_index();
                lat = sector.get_lat();
                lon = sector.get_lon();
            }
            if packet.push(measurement).is_err() {
                info!("[comm] Packet full, stopping adding measurements");
                break;
            }
        }
    }

    for i in 1..(num_measurements - sectors.len() as u32 + 1) {
        let index = highest_index - i % NUM_MEASUREMENTS as u32;
        if let Ok(measurement) = get_measurement_packet(storage, index).await {
            if packet.push(measurement).is_err() {
                info!("[comm] Packet full, stopping adding measurements");
                break;
            }
        }
    }

    // Save quantized values in packet
    packet.lat = coord_to_u8(lat, 1000.0);
    packet.lon = coord_to_u8(lon, 1000.0);

    let data = packet.to_bytes();
    info!("[comm] Sending packet with {} measurements, total size {} bytes", packet.measurements.len(), data.len());
    
    let start = Instant::now();
    let mut body = [0u8; BODY_SIZE];
    let data_slice = data.as_slice();
    let len = data_slice.len().min(BODY_SIZE);
    body[..len].copy_from_slice(&data_slice[..len]);
    let message = IMTMessage::new(
        IMT_DEFAULT_TOPIC,
        body,
        len as u8,
    );

    let result = rockblock.send_message(message).await;
    if result.is_none() {
        info!("[comm] RockBlock send failed after {} ms", (Instant::now() - start).as_millis());
        return Err(CommsError::RockBlockSendFail);
    }
    info!("[comm] Packet sent in {} ms", (Instant::now() - start).as_millis());

    info!("[comm] Turning off RockBlock");
    rockblock.power_off().await;
    info!("[comm] RockBlock powered off");

    Ok(())
}

async fn get_measurement_packet(storage: &'static StorageType, measurement_index: u32) -> Result<MeasurementPacket, CommsError> {
    let measurement_storage = MeasurementStorage::new();
    let measurement: Option<Measurement>;
    {
        let mut storage_lock = storage.lock().await;
        let storage = storage_lock.as_mut().ok_or(CommsError::StorageAccess)?;
        measurement = measurement_storage.read(storage, measurement_index);
    }
    if measurement.is_none() {
        info!("[comm] Failed to get measurement {}, skipping", measurement_index);
        return Err(CommsError::StorageAccess);
    }
    let measurement = measurement.unwrap();
    let packet_measurement = MeasurementPacket::new(
        (measurement.uid % 256) as u8,
        mean_f32_to_u16(measurement.mean, 20.0),
        std_f32_to_u8(measurement.std, 1.0),
        measurement.observations.len() as u8,
        measurement.num_seen as u8,
    );
    info!(
        "[comm] Read measurement {} (with {} observations, mean {}, std {})", 
        measurement_index,
        packet_measurement.num_observations_used,
        packet_measurement.relative_height_mean,
        packet_measurement.relative_height_std
    );
    Ok(packet_measurement)
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

fn coord_to_u8(value: f32, scale: f32) -> u8 {
    let i = (value * scale) - floorf(value * scale);

    (i * 255.0) as u8
}