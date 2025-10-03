use defmt::{info, Str};

use embassy_rp::{gpio, uart};
use heapless::{format, String};
use base64::{engine::general_purpose::STANDARD, Engine as _};

pub enum JSPRMethod {
    GET,
    PUT,
}

impl JSPRMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            JSPRMethod::GET => "GET",
            JSPRMethod::PUT => "PUT",
        }
    }
}

pub enum JSPRTarget {
    ApiVersion,
    SimInterface,
    HwInfo,
    SimStatus,
    OperationalState,
    MessageProvisioning,
    MessageOriginate,
    MessageOriginateSegment,
    MessageOriginateStatus,
    ConstellationState,
}

impl JSPRTarget {
    pub fn as_str(&self) -> &'static str {
        match self {
            JSPRTarget::ApiVersion => "apiVersion",
            JSPRTarget::SimInterface => "simInterface",
            JSPRTarget::HwInfo => "hwInfo",
            JSPRTarget::SimStatus => "simStatus",
            JSPRTarget::OperationalState => "operationalState",
            JSPRTarget::MessageProvisioning => "messageProvisioning",
            JSPRTarget::MessageOriginate => "messageOriginate",
            JSPRTarget::MessageOriginateSegment => "messageOriginateSegment",
            JSPRTarget::MessageOriginateStatus => "messageOriginateStatus",
            JSPRTarget::ConstellationState => "constellationState",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "apiVersion" => Some(JSPRTarget::ApiVersion),
            "simInterface" => Some(JSPRTarget::SimInterface),
            "hwInfo" => Some(JSPRTarget::HwInfo),
            "simStatus" => Some(JSPRTarget::SimStatus),
            "operationalState" => Some(JSPRTarget::OperationalState),
            "messageProvisioning" => Some(JSPRTarget::MessageProvisioning),
            "messageOriginate" => Some(JSPRTarget::MessageOriginate),
            "messageOriginateSegment" => Some(JSPRTarget::MessageOriginateSegment),
            "messageOriginateStatus" => Some(JSPRTarget::MessageOriginateStatus),
            "constellationState" => Some(JSPRTarget::ConstellationState),
            _ => None,
        }
    }
}

pub enum JSPRResultCode {
    Ok = 200,
    UnsolicitedMessage = 299,
}

impl JSPRResultCode {
    pub fn from_u16(code: u16) -> Option<Self> {
        match code {
            200 => Some(JSPRResultCode::Ok),
            299 => Some(JSPRResultCode::UnsolicitedMessage),
            _ => None,
        }
    }
}

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct JSPRGetApiVersionItem {
    major: u8,
    minor: u8,
    patch: u8,
}

#[derive(Debug, Deserialize)]
pub struct JSPRGetApiVersion {
    supported_versions: [JSPRGetApiVersionItem; 5],
    active_version: JSPRGetApiVersionItem,
}

#[derive(Debug, Deserialize)]
pub struct JSPRGetSimInterface {
    interface: String<16>,
}

#[derive(Debug, Deserialize)]
pub struct JSPRGetOperationalState {
    reason: u8,
    state: String<16>,
}

#[derive(Debug, Deserialize)]
pub struct JSPRGetMessageProvisioningItem {
    topic_id: u16,
    topic_name: String<32>,
    priority: String<16>,
}

#[derive(Debug, Deserialize)]
pub struct JSPRGetMessageProvisioning {
    provisioning: [JSPRGetMessageProvisioningItem; 32],
}

#[derive(Debug, Deserialize)]
pub struct JSPRPutMessageOriginate {
    topic_id: u16,
    reference: u8,
    message_response: String<32>,
    message_id: u8,
}

#[derive(Debug, Deserialize)]
pub struct JSPRPutMessageOriginateSegment {
    topic_id: u16,
    segment_length: u16,
    segment_start: u16,
    message_id: u8,
}

#[derive(Debug, Deserialize)]
pub struct JSPRPutMessageOriginateStatus {
    topic_id: u16,
    message_id: u8,
    status: String<32>,
}

#[derive(Debug, Deserialize)]
pub struct JSPRGetConstellationState {
    constellation_visible: bool,
    signal_bars: u8,
}

#[derive(Debug, Deserialize)]
pub struct JSPRGetHwInfo {
    hw_version: String<8>,
    serial_number: String<8>,
    imei: String<16>,
    board_temp: f32,
}

#[derive(Debug, Deserialize)]
pub struct JSPRGetSimStatus {
    card_present: bool,
    sim_connected: bool,
    iccid: String<32>,
}

#[derive(Debug)]
pub enum RockBlockError {
    Timeout,
    InvalidResponse,
    CommunicationError,
}

const REQUEST_SIZE: usize = 256;
const RESPONSE_SIZE: usize = 256;
pub const BODY_SIZE: usize = 256;

pub const IMT_DEFAULT_TOPIC: u16 = 244;

pub struct IMTMessage {
    body: [u8; BODY_SIZE],
    length: u8,
    topic: u16,
}

impl IMTMessage {
    pub fn new(topic: u16, body: [u8; BODY_SIZE], length: u8) -> Self {
        Self {
            body,
            length,
            topic,
        }
    }
}

#[derive(PartialEq)]
pub enum RockBlock9704Status {
    Off,
    Unchecked,
    Checking,
    Ready,
    Transmitting,
    Receiving,
    Error,
}

pub struct RockBlock9704 {
    uart: uart::Uart<'static, uart::Async>,
    pin_power_enable: gpio::Output<'static>,
    pin_iridium_enable: gpio::Output<'static>,
    pin_iridium_status: gpio::Input<'static>,
    message_reference: u8,
    pub status: RockBlock9704Status,
}

impl RockBlock9704 {
    pub fn new(
        uart: uart::Uart<'static, uart::Async>,
        pin_power_enable: gpio::Output<'static>,
        pin_iridium_enable: gpio::Output<'static>,
        pin_iridium_status: gpio::Input<'static>,
    ) -> Self {
        Self {
            uart,
            pin_power_enable,
            pin_iridium_enable,
            pin_iridium_status,
            message_reference: 0,
            status: RockBlock9704Status::Off,
        }
    }

    pub async fn send_message(&mut self, mut message: IMTMessage) -> Option<()> {
        if self.status != RockBlock9704Status::Ready {
            info!("[ROCK] RockBlock 9704 not ready to send message");
            return None;
        }

        let provisioning = self.get_message_provisioning().await;
        let topic_provisioned = if let Some(prov) = provisioning {
            prov.provisioning.iter().any(|item| item.topic_id == message.topic)
        } else {
            false
        };

        if !topic_provisioned {
            info!("[ROCK] Topic {} not provisioned", message.topic);
            return None;
        }

        let crc = crc16::State::<crc16::XMODEM>::calculate(&message.body[..message.length as usize]);
        // Append CRC (big-endian) to the end of the message body
        let len = message.length as usize;
        if len + 2 > message.body.len() {
            info!("[ROCK] Message too long to append CRC");
            return None;
        }
        message.body[len] = (crc >> 8) as u8;
        message.body[len + 1] = (crc & 0xFF) as u8;
        message.length += 2;

        if let Some(response) = self.put_message_originate(message.topic, message.length).await {
            info!("[ROCK] Message sent with ID: {}, Reference: {}, Response: {}", response.message_id, response.reference, response.message_response.as_str());
        } else {
            info!("[ROCK] Failed to send message");
            return None;
        }

        self.status = RockBlock9704Status::Transmitting;

        loop {
            let mut buffer = [0u8; RESPONSE_SIZE];
            let (code, target, length) = self.receive_jspr(&mut buffer).await.expect("Failed to receive JSPR response");

            match target {
                JSPRTarget::MessageOriginateStatus => {
                    let (status_response, _) = serde_json_core::from_slice::<JSPRPutMessageOriginateStatus>(&buffer[..length as usize]).expect("Failed to parse MessageOriginateStatus response");
                    info!("[ROCK] Message Status: Topic ID: {}, Message ID: {}, Status: {}", status_response.topic_id, status_response.message_id, status_response.status.as_str());

                    if status_response.status.as_str() != "mo_ack_received" {
                        self.status = RockBlock9704Status::Error;
                        return None;
                    }
                },
                JSPRTarget::MessageOriginateSegment => {
                    let (segment_response, _) = serde_json_core::from_slice::<JSPRPutMessageOriginateSegment>(&buffer[..length as usize]).expect("Failed to parse MessageOriginateSegment response");
                    info!("[ROCK] Message Segment: Topic ID: {}, Message ID: {}, Segment Start: {}, Segment Length: {}", segment_response.topic_id, segment_response.message_id, segment_response.segment_start, segment_response.segment_length);
                    let mut new_buffer = [0u8; BODY_SIZE*2];
                    let data = &message.body[segment_response.segment_start as usize .. (segment_response.segment_start + segment_response.segment_length) as usize];
                    let new_length = STANDARD.encode_slice(data, &mut new_buffer).expect("Failed to encode base64 segment");
                    self.put_message_originate_segment(
                        message.topic, 
                        segment_response.message_id as u16, 
                        segment_response.segment_start, 
                        segment_response.segment_length,
                        &core::str::from_utf8(&new_buffer[..new_length]).expect("Failed to convert segment to string")
                    ).await;
                },
                JSPRTarget::ConstellationState => {
                    let (constellation_response, _) = serde_json_core::from_slice::<JSPRGetConstellationState>(&buffer[..length as usize]).expect("Failed to parse ConstellationState response");
                    info!("[ROCK] Constellation State: Visible: {}, Signal Bars: {}", constellation_response.constellation_visible, constellation_response.signal_bars);
                },
                _ => {
                    info!("[ROCK] Unexpected JSPR Target: {}", target.as_str());
                }
            }
        }

        self.status = RockBlock9704Status::Ready;
        Some(())
    }

    pub async fn power_on(&mut self) {
        if self.status == RockBlock9704Status::Off {
            info!("[ROCK] Powering on RockBlock 9704");
            self.pin_power_enable.set_low();
            self.pin_iridium_enable.set_high();
            loop {
                if self.pin_iridium_status.is_high() {
                    break;
                }
            }
            self.status = RockBlock9704Status::Unchecked;
            info!("[ROCK] RockBlock 9704 powered on");
        } else {
            info!("[ROCK] RockBlock 9704 already powered on");
        }
    }

    pub async fn power_off(&mut self) {
        if self.status == RockBlock9704Status::Off {
            info!("[ROCK] RockBlock 9704 already powered off");
        } else {
            info!("[ROCK] Powering off RockBlock 9704");
            self.pin_iridium_enable.set_low();
            self.pin_power_enable.set_high();
            loop {
                if self.pin_iridium_status.is_low() {
                    break;
                }
            }
            self.status = RockBlock9704Status::Off;
            info!("[ROCK] RockBlock 9704 powered off");
        }
    }

    pub async fn check_status(&mut self) {
        if self.status != RockBlock9704Status::Unchecked {
            info!("RockBlock 9704 is either off, busy, in error state, or already ready");
            return;
        }

        self.status = RockBlock9704Status::Checking;

        if !self.valid_api_version().await {
            panic!("RockBlock 9704 not responding or invalid API version");
        }
        if !self.valid_sim_interface().await {
            panic!("RockBlock 9704 SIM interface not set to internal");
            // code sets sim interface to internal
        }
        if !self.valid_operational_state().await {
            panic!("RockBlock 9704 not in active state");
            // code sets to active state if inactive
            // or to inactive and then active for other states
        }

        self.status = RockBlock9704Status::Ready;
        info!("[ROCK] RockBlock 9704 is ready");
    }

    pub async fn valid_api_version(&mut self) -> bool {
        if let Some(data) = self.get_api_version().await {
            let active_version = &data.active_version;
            info!("[ROCK] API Version: {}.{}.{}", active_version.major, active_version.minor, active_version.patch);
            true
        } else {
            info!("[ROCK] Failed to get API Version");
            false
        }
    }

    pub async fn valid_sim_interface(&mut self) -> bool {
        if let Some(data) = self.get_sim_interface().await {
            info!("[ROCK] SIM Interface: {}", data.interface.as_str());
            data.interface.as_str() == "internal"
        } else {
            info!("[ROCK] Failed to get SIM Interface");
            false
        }
    }

    pub async fn valid_operational_state(&mut self) -> bool {
        if let Some(data) = self.get_operational_state().await {
            info!("[ROCK] Operational State: {} (reason {})", data.state.as_str(), data.reason);
            data.state.as_str() == "active"
        } else {
            info!("[ROCK] Failed to get Operational State");
            false
        }
    }

    pub async fn get_api_version(&mut self) -> Option<JSPRGetApiVersion> {
        // send_jspr("GET apiVersion {}\r")
        let mut message: [u8; RESPONSE_SIZE] = [0; RESPONSE_SIZE];
        let _ = self.fetch_jspr(JSPRMethod::GET, JSPRTarget::ApiVersion, "{}", &mut message).await.ok()?;
        let (response, _) = serde_json_core::from_slice::<JSPRGetApiVersion>(&message).ok()?;
        
        Some(response)
    }

    pub async fn get_sim_interface(&mut self) -> Option<JSPRGetSimInterface> {
        // send_jspr("GET simInterface {}\r")
        let mut message: [u8; RESPONSE_SIZE] = [0; RESPONSE_SIZE];
        let _ = self.fetch_jspr(JSPRMethod::GET, JSPRTarget::SimInterface, "{}", &mut message).await.ok()?;
        let (response, _) = serde_json_core::from_slice::<JSPRGetSimInterface>(&message).ok()?;
        
        Some(response)
    }

    pub async fn put_sim_interface(&mut self, interface: &str) -> Option<JSPRGetSimInterface> {
        // send_jspr("PUT simInterface {\"interface\":\"internal\"}\r")
        let body = format!(64; "{{\"interface\":\"{}\"}}", interface).expect("Failed to format body");
        let mut message: [u8; RESPONSE_SIZE] = [0; RESPONSE_SIZE];
        let (_, _, _) = self.fetch_jspr(JSPRMethod::PUT, JSPRTarget::SimInterface, &body, &mut message).await.ok()?;
        let (response, _) = serde_json_core::from_slice::<JSPRGetSimInterface>(&message).ok()?;
        
        Some(response)
    }

    pub async fn get_operational_state(&mut self) -> Option<JSPRGetOperationalState> {
        // send_jspr("GET operationalState {}\r")
        let mut message: [u8; RESPONSE_SIZE] = [0; RESPONSE_SIZE];
        let _ = self.fetch_jspr(JSPRMethod::GET, JSPRTarget::OperationalState, "{}", &mut message).await.ok()?;
        let (response, _) = serde_json_core::from_slice::<JSPRGetOperationalState>(&message).ok()?;
        
        Some(response)
    }

    pub async fn get_message_provisioning(&mut self) -> Option<JSPRGetMessageProvisioning> {
        // send_jspr("GET messageProvisioning {}\r")
        let mut message: [u8; RESPONSE_SIZE] = [0; RESPONSE_SIZE];
        let _ = self.fetch_jspr(JSPRMethod::GET, JSPRTarget::MessageProvisioning, "{}", &mut message).await.ok()?;
        let (response, _) = serde_json_core::from_slice::<JSPRGetMessageProvisioning>(&message).ok()?;
        
        Some(response)
    }

    pub async fn put_message_originate(&mut self, topic: u16, length: u8) -> Option<JSPRPutMessageOriginate> {
        self.message_reference = self.message_reference.wrapping_add(1);
        let body = format!(128; "{{\"topic_id\":{},\"message_length\":{},\"request_reference\":{}}}", topic, length, self.message_reference).expect("Failed to format body");
        self.message_reference = (self.message_reference + 1) % 100;
        let mut message: [u8; RESPONSE_SIZE] = [0; RESPONSE_SIZE];
        let (_, _, _) = self.fetch_jspr(JSPRMethod::PUT, JSPRTarget::MessageOriginate, &body, &mut message).await.ok()?;
        let (response, _) = serde_json_core::from_slice::<JSPRPutMessageOriginate>(&message).ok()?;

        Some(response)
    }

    pub async fn put_message_originate_segment(&mut self, topic: u16, message_id: u16, segment_start: u16, segment_length: u16, data: &str) -> Option<JSPRPutMessageOriginateSegment> {
        let body = format!({128*3}; "{{\"topic_id\":{},\"message_id\":{},\"segment_start\":{},\"segment_length\":{},\"data\":\"{}\"}}", topic, message_id, segment_start, segment_length, data).expect("Failed to format body");
        let mut message: [u8; RESPONSE_SIZE] = [0; RESPONSE_SIZE];
        let (_, _, _) = self.fetch_jspr(JSPRMethod::PUT, JSPRTarget::MessageOriginateSegment, &body, &mut message).await.ok()?;
        let (response, _) = serde_json_core::from_slice::<JSPRPutMessageOriginateSegment>(&message).ok()?;

        Some(response)
    }

    pub async fn get_constellation_state(&mut self) -> Option<JSPRGetConstellationState> {
        // send_jspr("GET constellationState {}\r")
        let mut message: [u8; RESPONSE_SIZE] = [0; RESPONSE_SIZE];
        let _ = self.fetch_jspr(JSPRMethod::GET, JSPRTarget::ConstellationState, "{}", &mut message).await.ok()?;
        let (response, _) = serde_json_core::from_slice::<JSPRGetConstellationState>(&message).ok()?;
        
        Some(response)
    }

    pub async fn get_hw_info(&mut self) -> Option<JSPRGetHwInfo> {
        // send_jspr("GET hwInfo {}\r")
        let mut message: [u8; RESPONSE_SIZE] = [0; RESPONSE_SIZE];
        let _ = self.fetch_jspr(JSPRMethod::GET, JSPRTarget::HwInfo, "{}", &mut message).await.ok()?;
        let (response, _) = serde_json_core::from_slice::<JSPRGetHwInfo>(&message).ok()?;
        
        Some(response)
    }

    pub async fn get_sim_status(&mut self) -> Option<JSPRGetSimStatus> {
        // send_jspr("GET simStatus {}\r")
        let mut message: [u8; RESPONSE_SIZE] = [0; RESPONSE_SIZE];
        let _ = self.fetch_jspr(JSPRMethod::GET, JSPRTarget::SimStatus, "{}", &mut message).await.ok()?;
        let (response, _) = serde_json_core::from_slice::<JSPRGetSimStatus>(&message).ok()?;
        
        Some(response)
    }

    pub async fn fetch_jspr(
        &mut self, 
        method: JSPRMethod,
        target: JSPRTarget,
        body: &str,
        message: &mut [u8]
    ) -> Result<(JSPRResultCode, JSPRTarget, u8), RockBlockError> {
        self.send_jspr(method, target, body).await;
        self.receive_jspr(message).await
    }

    pub async fn send_jspr(
        &mut self, 
        method: JSPRMethod,
        target: JSPRTarget,
        body: &str
    ) {
        let request = String::<256>::from(format!("{} {} {}\r", method.as_str(), target.as_str(), body).expect("Failed to format request"));

        info!("[ROCK] Sending: {}", request.as_str());
        self.uart.write(request.as_bytes()).await.expect("Failed to write request");
    }

    pub async fn receive_jspr(
        &mut self, 
        message: &mut [u8]
    ) -> Result<(JSPRResultCode, JSPRTarget, u8), RockBlockError> {
        let mut buffer: [u8; RESPONSE_SIZE] = [0; RESPONSE_SIZE];
        let mut index: usize = 0;
        loop {
            let mut byte: [u8; 1] = [0; 1];
            match self.uart.read(&mut byte).await {
                Ok(_) => {
                    if byte[0] == b'\r' || index >= buffer.len() {
                        break;
                    }
                    buffer[index] = byte[0];
                    index += 1;
                },
                Err(_) => {
                    // handle error
                    break;
                }
            }
        }
        let response = core::str::from_utf8(&buffer[..index]).expect("Failed to parse response");
        info!("[ROCK] Received: {}", response);

        let mut parts = response.splitn(3, ' ');

        let result_code_str = parts.next().ok_or(RockBlockError::InvalidResponse)?;
        let result_code = result_code_str.parse::<u16>().map_err(|_| RockBlockError::InvalidResponse)?;
        let result_code = JSPRResultCode::from_u16(result_code).ok_or(RockBlockError::InvalidResponse)?;
       
        let target_str = parts.next().ok_or(RockBlockError::InvalidResponse)?;
        let target = JSPRTarget::from_str(target_str).ok_or(RockBlockError::InvalidResponse)?;

        let body_str = parts.next().unwrap_or("");
        message[..body_str.len()].copy_from_slice(body_str.as_bytes());

        Ok((result_code, target, body_str.len() as u8))
    }
}