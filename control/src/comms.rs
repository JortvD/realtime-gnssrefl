
use defmt::info;
use embassy_futures::select::{select, Either};
use embassy_time::{Instant, Timer};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use heapless::Vec;

use crate::{ control::{CommReqMsg, CommResMsg, MAX_SECTORS}, rockblock::{IMTMessage, RockBlock9704, BODY_SIZE, IMT_DEFAULT_TOPIC}, storage::MeasurementStorage, types::{Measurement, Sector, BLOCK_SIZE, NUM_BINS, NUM_CONTAINER_BLOCKS, NUM_MEASUREMENTS}, StorageType};

pub struct MeasurementPacket { // 5 bytes
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

pub struct Packet { // 5 + n * 5 bytes = 55
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
        let mut data = Vec::<u8, {5 * MAX_MEASUREMENT_PACKETS + 5}>::new(); // 55 bytes
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
pub async fn task_comms(
    channel_req: &'static Channel<CriticalSectionRawMutex, CommReqMsg, 8>,
    channel_res: &'static Channel<CriticalSectionRawMutex, CommResMsg, 8>,
    storage: &'static StorageType,
    mut rockblock: RockBlock9704,
) {
    loop {
        info!("[comm] waiting for request");
        run_comms_test(&mut rockblock).await;
        let select = select(
            channel_req.receive(), 
            Timer::after_secs(u64::MAX)
        ).await;
        match select {
            Either::First(CommReqMsg::Send { sectors, config }) => {
                let sector = sectors.get(0).expect("At least one sector should be present");
                info!("[comm] starting communication for sector {}", sector.get_measurement_index());
                run_comms(&mut rockblock, storage, sector, config.num_send_measurements).await;
                let mut uids = Vec::<u32, MAX_SECTORS>::new();
                uids.push(sector.get_uid()).ok();
                channel_res.send(CommResMsg::Success { sector_uids: uids }).await;
            }
            Either::Second(command) => {
                info!("[comm] rockblock command received");
                // match command.ok().expect("msg") {
                //     RockBlockCommand::DMP1 => dump_bin_storage(&mut rockblock, storage).await,
                //     _ => {},
                // }
            }
        }
    }
}

async fn run_comms_test(rockblock: &mut RockBlock9704) {
    info!("[comm] Turning on RockBlock for test");
    rockblock.power_on().await;
    if rockblock.status != crate::rockblock::RockBlock9704Status::Unchecked {
        info!("[comm] RockBlock not ready to be checked, aborting comms test");
        return;
    }
    info!("[comm] Checking RockBlock status for test");
    rockblock.check_status().await;
    if rockblock.status != crate::rockblock::RockBlock9704Status::Ready {
        info!("[comm] RockBlock not ready, aborting comms test");
        return;
    }
    info!("[comm] RockBlock ready, sending test message");

    for _ in 0..5 {
        let response = rockblock.get_constellation_state().await;
        if let Some(response) = response {
            info!("[comm] Get constellation response: {}, {}, body length {}", response.constellation_visible, response.signal_level, response.signal_bars);
        } else {
            info!("[comm] Get constellation message failed");
        }
        Timer::after_secs(5).await;
    }

    let mut body = [0u8; 256];
    let hello = b"Wie t leest trekt een bak";
    body[..hello.len()].copy_from_slice(hello);

    let message = IMTMessage::new(
        IMT_DEFAULT_TOPIC,
        body,
        hello.len() as u8,
    );

    rockblock.send_message(message).await;
    info!("[comm] Test message sent");

    info!("[comm] Turning off RockBlock after test");
    rockblock.power_off().await;
    info!("[comm] RockBlock powered off after test");
}

async fn dump_bin_storage(rockblock: &mut RockBlock9704, storage: &'static StorageType) {
    let mut storage_lock = storage.lock().await;
    let storage = storage_lock.as_mut().expect("Storage not initialized");

    for bin in 0..NUM_BINS {
        for block in 0..NUM_CONTAINER_BLOCKS {
            let mut io_buf = [0u8; 256];

            storage.read(bin, (block * BLOCK_SIZE) as u32, &mut io_buf).expect("");

            // let message = IMTMessage::new(
            //     IMT_DEFAULT_TOPIC,
            //     io_buf,
            //     BLOCK_SIZE as u8,
            // );

            // rockblock.send_message(message).await;
        }
    }
}

async fn run_comms(
    rockblock: &mut RockBlock9704,
    storage: &'static StorageType,
    sector: &Sector,
    num_measurements: u32,
) {
    info!("[comm] Turning on RockBlock");
    rockblock.power_on().await;
    if rockblock.status != crate::rockblock::RockBlock9704Status::Unchecked {
        info!("[comm] RockBlock not ready to be checked, aborting comms");
        return;
    }
    info!("[comm] Checking RockBlock status");
    rockblock.check_status().await;
    if rockblock.status != crate::rockblock::RockBlock9704Status::Ready {
        info!("[comm] RockBlock not ready, aborting comms");
        return;
    }
    info!("[comm] RockBlock ready, preparing packet");

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
            measurement = measurement_storage.read(storage, (sector.get_measurement_index() - i) % NUM_MEASUREMENTS as u32);
        }
        if measurement.is_none() {
            info!("[comm] No measurement found at location {}, skipping", (sector.get_measurement_index() - i) % NUM_MEASUREMENTS as u32);
            continue;
        }
        let measurement = measurement.unwrap();
        info!("[comm] Read measurement at location {} with {} observations, mean {}, std {}", (sector.get_measurement_index() - i) % NUM_MEASUREMENTS as u32, measurement.observations.len(), measurement.mean, measurement.std);
        let packet_measurement = MeasurementPacket::new(
            mean_f32_to_u16(measurement.mean, 20.0),
            std_f32_to_u8(measurement.std, 1.0),
            measurement.observations.len() as u8,
            measurement.num_seen as u8,
        );
        packet.push(packet_measurement).ok();
    }

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

    rockblock.send_message(message).await;
    info!("[comm] Packet sent in {} ms", (Instant::now() - start).as_millis());

    info!("[comm] Turning off RockBlock");
    rockblock.power_off().await;
    info!("[comm] RockBlock powered off");
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