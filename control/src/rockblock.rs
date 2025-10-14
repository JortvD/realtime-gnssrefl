use defmt::info;

use embassy_futures::select::{select, Either};
use embassy_rp::{gpio, pac::Interrupt::PIO0_IRQ_1, uart};
use embassy_time::Timer;
use heapless::{format, String, Vec};
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

#[derive(Debug, PartialEq)]
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
    MessageTerminate,
    MessageTerminateSegment,
    MessageTerminateStatus,
    ConstellationState,
}

impl JSPRTarget {
    pub fn as_str(&self) -> &'static str {
        match self {
            JSPRTarget::ApiVersion => "apiVersion",
            JSPRTarget::SimInterface => "simConfig",
            JSPRTarget::HwInfo => "hwInfo",
            JSPRTarget::SimStatus => "simStatus",
            JSPRTarget::OperationalState => "operationalState",
            JSPRTarget::MessageProvisioning => "messageProvisioning",
            JSPRTarget::MessageOriginate => "messageOriginate",
            JSPRTarget::MessageOriginateSegment => "messageOriginateSegment",
            JSPRTarget::MessageOriginateStatus => "messageOriginateStatus",
            JSPRTarget::MessageTerminate => "messageTerminate",
            JSPRTarget::MessageTerminateSegment => "messageTerminateSegment",
            JSPRTarget::MessageTerminateStatus => "messageTerminateStatus",
            JSPRTarget::ConstellationState => "constellationState",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "apiVersion" => Some(JSPRTarget::ApiVersion),
            "simConfig" => Some(JSPRTarget::SimInterface),
            "hwInfo" => Some(JSPRTarget::HwInfo),
            "simStatus" => Some(JSPRTarget::SimStatus),
            "operationalState" => Some(JSPRTarget::OperationalState),
            "messageProvisioning" => Some(JSPRTarget::MessageProvisioning),
            "messageOriginate" => Some(JSPRTarget::MessageOriginate),
            "messageOriginateSegment" => Some(JSPRTarget::MessageOriginateSegment),
            "messageOriginateStatus" => Some(JSPRTarget::MessageOriginateStatus),
            "messageTerminate" => Some(JSPRTarget::MessageTerminate),
            "messageTerminateSegment" => Some(JSPRTarget::MessageTerminateSegment),
            "messageTerminateStatus" => Some(JSPRTarget::MessageTerminateStatus),
            "constellationState" => Some(JSPRTarget::ConstellationState),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum JSPRResultCode {
    Ok = 200,
    UnsolicitedMessage = 299,
    VersionNotSelected = 400,
    UnsupportedRequestType = 401,
    ConfigurationAlreadySet = 402,
    CommandTooLong = 403,
    UnknownTarget = 404,
    CommandMalformed = 405,
    OperationNotAllowed = 406,
    BadJson = 407,
    RequestFailed = 408,
    Unauthorized = 409,
    SimNotConfigured = 410,
    WakeXcvrInInvalid = 411,
    InvalidChannel = 412,
    InvalidAction = 413,
    HardwareNotConfigured = 414,
    InvalidRadioPath = 415,
    CrashDumpNotAvailable = 416,
    FeatureNotSupportedByHardware = 417,
    NotProvisioned = 418,
    InvalidTransmitPower = 419,
    InvalidBurstType = 420,
    SerialPortError = 500,
}

impl JSPRResultCode {
    pub fn from_u16(code: u16) -> Option<Self> {
        match code {
            200 => Some(JSPRResultCode::Ok),
            299 => Some(JSPRResultCode::UnsolicitedMessage),
            400 => Some(JSPRResultCode::VersionNotSelected),
            401 => Some(JSPRResultCode::UnsupportedRequestType),
            402 => Some(JSPRResultCode::ConfigurationAlreadySet),
            403 => Some(JSPRResultCode::CommandTooLong),
            404 => Some(JSPRResultCode::UnknownTarget),
            405 => Some(JSPRResultCode::CommandMalformed),
            406 => Some(JSPRResultCode::OperationNotAllowed),
            407 => Some(JSPRResultCode::BadJson),
            408 => Some(JSPRResultCode::RequestFailed),
            409 => Some(JSPRResultCode::Unauthorized),
            410 => Some(JSPRResultCode::SimNotConfigured),
            411 => Some(JSPRResultCode::WakeXcvrInInvalid),
            412 => Some(JSPRResultCode::InvalidChannel),
            413 => Some(JSPRResultCode::InvalidAction),
            414 => Some(JSPRResultCode::HardwareNotConfigured),
            415 => Some(JSPRResultCode::InvalidRadioPath),
            416 => Some(JSPRResultCode::CrashDumpNotAvailable),
            417 => Some(JSPRResultCode::FeatureNotSupportedByHardware),
            418 => Some(JSPRResultCode::NotProvisioned),
            419 => Some(JSPRResultCode::InvalidTransmitPower),
            420 => Some(JSPRResultCode::InvalidBurstType),
            500 => Some(JSPRResultCode::SerialPortError),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            JSPRResultCode::Ok => "200 OK",
            JSPRResultCode::UnsolicitedMessage => "299 Unsolicited Message",
            JSPRResultCode::VersionNotSelected => "400 Version Not Selected",
            JSPRResultCode::UnsupportedRequestType => "401 Unsupported Request Type",
            JSPRResultCode::ConfigurationAlreadySet => "402 Configuration Already Set",
            JSPRResultCode::CommandTooLong => "403 Command Too Long",
            JSPRResultCode::UnknownTarget => "404 Unknown Target",
            JSPRResultCode::CommandMalformed => "405 Command Malformed",
            JSPRResultCode::OperationNotAllowed => "406 Operation Not Allowed",
            JSPRResultCode::BadJson => "407 Bad JSON",
            JSPRResultCode::RequestFailed => "408 Request Failed",
            JSPRResultCode::Unauthorized => "409 Unauthorized",
            JSPRResultCode::SimNotConfigured => "410 SIM Not Configured",
            JSPRResultCode::WakeXcvrInInvalid => "411 Wake Xcvr Invalid",
            JSPRResultCode::InvalidChannel => "412 Invalid Channel",
            JSPRResultCode::InvalidAction => "413 Invalid Action",
            JSPRResultCode::HardwareNotConfigured => "414 Hardware Not Configured",
            JSPRResultCode::InvalidRadioPath => "415 Invalid Radio Path",
            JSPRResultCode::CrashDumpNotAvailable => "416 Crash Dump Not Available",
            JSPRResultCode::FeatureNotSupportedByHardware => "417 Feature Not Supported By Hardware",
            JSPRResultCode::NotProvisioned => "418 Not Provisioned",
            JSPRResultCode::InvalidTransmitPower => "419 Invalid Transmit Power",
            JSPRResultCode::InvalidBurstType => "420 Invalid Burst Type",
            JSPRResultCode::SerialPortError => "500 Serial Port Error",
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
    supported_versions: Vec<JSPRGetApiVersionItem, 3>,
    active_version: Option<JSPRGetApiVersionItem>,
}

#[derive(Debug, Deserialize)]
pub struct JSPRGetSimInterface {
    interface: String<16>,
}

#[derive(Debug, Deserialize)]
pub struct JSPRGetOperationalState {
    reason: Option<u8>,
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
    provisioning: Vec::<JSPRGetMessageProvisioningItem, 32>,
}

#[derive(Debug, Deserialize)]
pub struct JSPRPutMessageOriginate {
    topic_id: u16,
    request_reference: u8,
    message_response: String<32>,
    message_id: u8,
}

#[derive(Debug, Deserialize)]
pub struct JSPRPutMessageOriginateSegment {
    topic_id: u16,
    segment_length: u16,
    segment_start: u32,
    message_id: u8,
}

#[derive(Debug, Deserialize)]
pub struct JSPRPutMessageOriginateStatus {
    topic_id: u16,
    message_id: u8,
    final_mo_status: String<32>,
}

#[derive(Debug, Deserialize)]
pub struct JSPRUnsMessageTerminateSegment {
    topic_id: u16,
    message_id: u8,
    segment_start: u16,
    segment_length: u32,
    data: String<512>,
}

#[derive(Debug, Deserialize)]
pub struct JSPRUnsMessageTerminate {
    topic_id: u16,
    message_id: u8,
    message_length_max: u32,
}

#[derive(Debug, Deserialize)]
pub struct JSPRUnsMessageTerminateStatus {
    topic_id: u16,
    message_id: u8,
    final_mt_status: String<32>,
}

#[derive(Debug, Deserialize)]
pub struct JSPRGetConstellationState {
    pub constellation_visible: bool,
    pub signal_level: Option<i8>,
    pub signal_bars: u8,
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
    ReceiveOverflow,
}

const REQUEST_SIZE: usize = 256;
const RESPONSE_SIZE: usize = 256;
pub const BODY_SIZE: usize = 256;
pub const MAX_RESPONSE_SIZE: usize = 256;
const MAX_SEND_ITERATIONS: usize = 100;
const MAX_RECEIVE_ITERATIONS: usize = 100;
const MAX_POWER_ON_ITERATIONS: usize = 100;

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
    debug: bool,
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
            debug: true,
        }
    }

    pub async fn send_message(&mut self, mut message: IMTMessage) -> Option<()> {
        if self.status != RockBlock9704Status::Ready {
            if self.debug { info!("[ROCK] RockBlock 9704 not ready to send message"); }
            return None;
        }

        // let provisioning = self.get_message_provisioning().await;
        // let topic_provisioned = if let Some(prov) = provisioning {
        //     prov.provisioning.iter().any(|item| item.topic_id == message.topic)
        // } else {
        //     false
        // };

        // if !topic_provisioned {
        //     if self.debug { info!("[ROCK] Topic {} not provisioned", message.topic); }
        //     return None;
        // }

        let crc = crc16::State::<crc16::XMODEM>::calculate(&message.body[..message.length as usize]);
        // Append CRC (big-endian) to the end of the message body
        let len = message.length as usize;
        if len + 2 > message.body.len() {
            if self.debug { info!("[ROCK] Message too long to append CRC"); }
            return None;
        }
        message.body[len] = (crc >> 8) as u8;
        message.body[len + 1] = (crc & 0xFF) as u8;
        message.length += 2;

        if let Some(response) = self.put_message_originate(message.topic, message.length).await {
            if self.debug { info!("[ROCK] Message sent with Topic ID: {}, Message ID: {}, Request Reference: {}, Response: {}", response.topic_id, response.message_id, response.request_reference, response.message_response.as_str()); }

            if response.message_response.as_str() != "message_accepted" {
                if self.debug { info!("[ROCK] Message not accepted by RockBlock"); }
                return None;
            }
        } else {
            if self.debug { info!("[ROCK] Failed to send message"); }
            return None;
        }

        self.status = RockBlock9704Status::Transmitting;
        let mut i = 0;

        loop {
            let mut buffer = [0u8; RESPONSE_SIZE];
            let (code, target, length) = self.receive_jspr(&mut buffer, true).await.expect("Failed to receive JSPR response");

            if code != JSPRResultCode::UnsolicitedMessage {
                if self.debug { info!("[ROCK][{:02}] JSPR Error: {}", i, code.as_str()); }
                self.status = RockBlock9704Status::Error;
                return None;
            }

            match target {
                JSPRTarget::MessageOriginateStatus => {
                    let (status_response, _) = serde_json_core::from_slice::<JSPRPutMessageOriginateStatus>(&buffer[..length as usize]).expect("Failed to parse MessageOriginateStatus response");
                    if self.debug { info!("[ROCK][{:02}] Message Status: Topic ID: {}, Message ID: {}, Status: {}", i, status_response.topic_id, status_response.message_id, status_response.final_mo_status.as_str()); }

                    if status_response.final_mo_status.as_str() != "mo_ack_received" {
                        self.status = RockBlock9704Status::Error;
                        return None;
                    }
                    break;
                },
                JSPRTarget::MessageOriginateSegment => {
                    let (segment_response, _) = serde_json_core::from_slice::<JSPRPutMessageOriginateSegment>(&buffer[..length as usize]).expect("Failed to parse MessageOriginateSegment response");
                    if self.debug { info!("[ROCK][{:02}] Message Segment: Topic ID: {}, Message ID: {}, Segment Start: {}, Segment Length: {}", i, segment_response.topic_id, segment_response.message_id, segment_response.segment_start, segment_response.segment_length); }
                    let mut new_buffer = [0u8; BODY_SIZE*2];
                    let data = &message.body[segment_response.segment_start as usize .. (segment_response.segment_start + segment_response.segment_length as u32) as usize];
                    let new_length = STANDARD.encode_slice(data, &mut new_buffer).expect("Failed to encode base64 segment");
                    let response = self.put_message_originate_segment(
                        message.topic, 
                        segment_response.message_id as u16, 
                        segment_response.segment_start, 
                        segment_response.segment_length,
                        &core::str::from_utf8(&new_buffer[..new_length]).expect("Failed to convert segment to string")
                    ).await?;
                    if self.debug { info!("[ROCK][{:02}] Sent segment, response topic_id {}, message_id: {}", i, response.topic_id, response.message_id); }
                },
                JSPRTarget::ConstellationState => {
                    let (constellation_response, _) = serde_json_core::from_slice::<JSPRGetConstellationState>(&buffer[..length as usize]).expect("Failed to parse ConstellationState response");
                    if self.debug { info!("[ROCK][{:02}] Constellation State: Visible: {}, Signal Level: {}, Signal Bars: {}", i, constellation_response.constellation_visible, constellation_response.signal_level, constellation_response.signal_bars); }
                },
                _ => {
                    if self.debug { info!("[ROCK][{:02}] Unexpected JSPR Target: {}", i, target.as_str()); }
                }
            }

            i += 1;

            if i > MAX_SEND_ITERATIONS {
                if self.debug { info!("[ROCK] Max iterations reached while sending message"); }
                self.status = RockBlock9704Status::Error;
                return None;
            }
        }

        self.status = RockBlock9704Status::Ready;
        Some(())
    }

    pub async fn receive_message(&mut self, buffer: &mut [u8; MAX_RESPONSE_SIZE]) -> Option<u16> {
        self.status = RockBlock9704Status::Receiving;
        let mut x = 0;
        let mut i = 0;

        loop {
            let mut buf = [0u8; RESPONSE_SIZE];
            let (code, target, length) = self.receive_jspr(&mut buf, true).await.expect("Failed to receive JSPR response");

            if code != JSPRResultCode::UnsolicitedMessage {
                if self.debug { info!("[ROCK][{:02}] JSPR Error: {}", i, code.as_str()); }
                self.status = RockBlock9704Status::Error;
                return None;
            }

            match target {
                JSPRTarget::MessageTerminate => {
                    let (terminate_response, _) = serde_json_core::from_slice::<JSPRUnsMessageTerminate>(&buf[..length as usize]).expect("Failed to parse MessageTerminate response");
                    if self.debug { info!("[ROCK][{:02}] Message Terminate: Topic ID: {}, Message ID: {}, Max Length: {}", i, terminate_response.topic_id, terminate_response.message_id, terminate_response.message_length_max); }
                },
                JSPRTarget::MessageTerminateSegment => {
                    let (segment_response, _) = serde_json_core::from_slice::<JSPRUnsMessageTerminateSegment>(&buf[..length as usize]).expect("Failed to parse MessageTerminateSegment response");
                    if self.debug { info!("[ROCK][{:02}] Message Segment: Topic ID: {}, Message ID: {}, Segment Start: {}, Segment Length: {}", i, segment_response.topic_id, segment_response.message_id, segment_response.segment_start, segment_response.segment_length); }
                    let mut temp = [0u8; BODY_SIZE*2];
                    let decoded_length = STANDARD.decode_slice(&segment_response.data.as_bytes(), &mut temp).ok()?;
                    buffer[x..x+decoded_length].copy_from_slice(&temp[..decoded_length]);
                    x += decoded_length;

                    if self.debug { info!("[ROCK][{:02}] Received {} bytes, total {}", i, decoded_length, x); }
                },
                JSPRTarget::MessageTerminateStatus => {
                    let (status_response, _) = serde_json_core::from_slice::<JSPRUnsMessageTerminateStatus>(&buf[..length as usize]).expect("Failed to parse MessageTerminateStatus response");
                    if self.debug { info!("[ROCK][{:02}] Message Status: Topic ID: {}, Message ID: {}, Status: {}", i, status_response.topic_id, status_response.message_id, status_response.final_mt_status.as_str()); }
                    if status_response.final_mt_status.as_str() != "complete" {
                        self.status = RockBlock9704Status::Error;
                        return None;
                    }
                    break;
                },
                JSPRTarget::ConstellationState => {
                    let (constellation_response, _) = serde_json_core::from_slice::<JSPRGetConstellationState>(&buf[..length as usize]).expect("Failed to parse ConstellationState response");
                    if self.debug { info!("[ROCK][{:02}] Constellation State: Visible: {}, Signal Level: {}, Signal Bars: {}", i, constellation_response.constellation_visible, constellation_response.signal_level, constellation_response.signal_bars); }
                },
                _ => {
                    if self.debug { info!("[ROCK][{:02}] Unexpected JSPR Target: {}", i, target.as_str()); }
                }
            }

            i += 1;

            if i > MAX_RECEIVE_ITERATIONS {
                if self.debug { info!("[ROCK] Max iterations reached while receiving message"); }
                self.status = RockBlock9704Status::Error;
                return None;
            }
        }
        self.status = RockBlock9704Status::Ready;
        Some(x as u16 - 2) // subtract 2 for CRC
    }

    pub async fn power_on(&mut self) {
        if self.status == RockBlock9704Status::Off {
            if self.debug { info!("[ROCK] Powering on RockBlock 9704"); }
            // self.pin_power_enable.set_low();
            self.pin_iridium_enable.set_high();
            loop {
                if self.pin_iridium_status.is_high() {
                    break;
                }
            }
            self.status = RockBlock9704Status::Unchecked;
            if self.debug { info!("[ROCK] RockBlock 9704 powered on"); }
        } else {
            if self.debug { info!("[ROCK] RockBlock 9704 already powered on"); }
        }
    }

    pub async fn power_off(&mut self) {
        if self.status == RockBlock9704Status::Off {
            if self.debug { info!("[ROCK] RockBlock 9704 already powered off"); }
        } else {
            if self.debug { info!("[ROCK] Powering off RockBlock 9704"); }
            self.pin_iridium_enable.set_low();
            self.pin_power_enable.set_high();
            let mut i = 0;
            loop {
                if self.pin_iridium_status.is_low() {
                    break;
                }
                Timer::after_millis(100).await;
                i += 1;
                if i > MAX_POWER_ON_ITERATIONS {
                    if self.debug { info!("[ROCK] Max iterations reached while powering off"); }
                    self.status = RockBlock9704Status::Error;
                    return;
                }
            }
            self.status = RockBlock9704Status::Off;
            if self.debug { info!("[ROCK] RockBlock 9704 powered off"); }
        }
    }

    pub async fn check_status(&mut self) {
        if self.status != RockBlock9704Status::Unchecked {
            if self.debug { info!("RockBlock 9704 is either off, busy, in error state, or already ready"); }
            return;
        }

        self.status = RockBlock9704Status::Checking;

        // self.send_break().await;

        Timer::after_secs(5).await;

        if !self.validate_api_version().await {
            panic!("RockBlock 9704 not responding or invalid API version");
        }

        Timer::after_secs(5).await;

        if !self.valid_sim_interface().await {
            panic!("RockBlock 9704 SIM interface not set to internal");
        }

        Timer::after_secs(5).await;

        if !self.valid_operational_state().await {
            panic!("RockBlock 9704 not in active state");
            // code sets to active state if inactive
            // or to inactive and then active for other states
        }

        self.status = RockBlock9704Status::Ready;
        if self.debug { info!("[ROCK] RockBlock 9704 is ready"); }
    }

    pub async fn validate_api_version(&mut self) -> bool {
        if let Some(data) = self.get_api_version().await {
            let active_version = &data.active_version;
            if let Some(version) = active_version {
                if self.debug { info!("[ROCK] Active API Version: {}.{}.{}", version.major, version.minor, version.patch); }
                true
            } else {
                if self.debug { info!("[ROCK] No active API version set"); }
                let supported_versions = &data.supported_versions;
                if let Some(version) = supported_versions.first() {
                    if self.debug { info!("[ROCK] Setting API Version to: {}.{}.{}", version.major, version.minor, version.patch); }
                    if let Some(new_version) = self.put_api_version(version.major, version.minor, version.patch).await {
                        if let Some(v) = &new_version.active_version {
                            if self.debug { info!("[ROCK] New Active API Version: {}.{}.{}", v.major, v.minor, v.patch); }
                            true
                        } else {
                            if self.debug { info!("[ROCK] Failed to set new API version"); }
                            false
                        }
                    } else {
                        if self.debug { info!("[ROCK] Failed to set new API version"); }
                        false
                    }
                } else {
                    if self.debug { info!("[ROCK] No supported API versions available"); }
                    false
                }
            }
        } else {
            if self.debug { info!("[ROCK] Failed to get API Version"); }
            false
        }
    }

    pub async fn valid_sim_interface(&mut self) -> bool {
        if let Some(data) = self.get_sim_interface().await {
            if self.debug { info!("[ROCK] SIM Interface: {}", data.interface.as_str()); }
            if data.interface.as_str() == "internal" {
                true
            } else {
                let (status, config) = self.put_sim_interface("internal").await;
                if let Some(status) = status {
                    if self.debug { info!("[ROCK] Changed SIM interface (present: {}, connected: {}, iccid: {})", status.card_present, status.sim_connected, status.iccid.as_str()); }
                    true
                } else if let Some(config) = config {
                    if self.debug { info!("[ROCK] Changed SIM Interface to: {}", config.interface.as_str()); }
                    config.interface.as_str() == "internal"
                } else {
                    if self.debug { info!("[ROCK] Failed to set SIM Interface to internal"); }
                    false
                }
            }
        } else {
            if self.debug { info!("[ROCK] Failed to get SIM Interface"); }
            false
        }
    }

    pub async fn valid_operational_state(&mut self) -> bool {
        if let Some(data) = self.get_operational_state().await {
            if self.debug { info!("[ROCK] Operational State: {} (reason {})", data.state.as_str(), data.reason); }
            if data.state.as_str() == "active" {
                true
            } else if data.state.as_str() == "inactive" {
                if let Some(new_state) = self.put_operational_state("active").await {
                    if self.debug { info!("[ROCK] Changed Operational State to active (reason {})", new_state.state.as_str()); }
                    true
                } else {
                    if self.debug { info!("[ROCK] Failed to set Operational State to active"); }
                    false
                }
            } else {
                false
            }
        } else {
            if self.debug { info!("[ROCK] Failed to get Operational State"); }
            false
        }
    }

    pub async fn get_api_version(&mut self) -> Option<JSPRGetApiVersion> {
        // send_jspr("GET apiVersion {}\r")
        let mut message: [u8; RESPONSE_SIZE] = [0; RESPONSE_SIZE];
        let (status, target, len) = self.fetch_jspr(JSPRMethod::GET, JSPRTarget::ApiVersion, "{}", &mut message).await.ok()?;

        if status != JSPRResultCode::Ok || target != JSPRTarget::ApiVersion {
            if self.debug { info!("[ROCK] Failed to get API version, status code: {} and received target: {}", status.as_str(), target.as_str()); }
            return None;
        }

        let result = serde_json_core::from_slice::<JSPRGetApiVersion>(&message[..len]);
        if result.is_err() {
            if self.debug { info!("[ROCK] Failed to parse API version response: {:?}", result.err()); }
            return None;
        }
        let response = result.ok()?.0;
        Some(response)
    }

    pub async fn put_api_version(&mut self, major: u8, minor: u8, patch: u8) -> Option<JSPRGetApiVersion> {
        // send_jspr("PUT apiVersion {\"major\":1,\"minor\":0,\"patch\":0}\r")
        let body = format!(64; "{{\"active_version\":{{\"major\":{},\"minor\":{},\"patch\":{}}}}}", major, minor, patch).expect("Failed to format body");
        let mut message: [u8; RESPONSE_SIZE] = [0; RESPONSE_SIZE];
        let (status, target, len) = self.fetch_jspr(JSPRMethod::PUT, JSPRTarget::ApiVersion, &body, &mut message).await.ok()?;

        if status != JSPRResultCode::Ok || target != JSPRTarget::ApiVersion {
            if self.debug { info!("[ROCK] Failed to put API version, status code: {}", status.as_str()); }
            return None;
        }

        let (response, _) = serde_json_core::from_slice::<JSPRGetApiVersion>(&message[..len]).ok()?;
        Some(response)
    }

    pub async fn get_sim_interface(&mut self) -> Option<JSPRGetSimInterface> {
        // send_jspr("GET simInterface {}\r")
        let mut message: [u8; RESPONSE_SIZE] = [0; RESPONSE_SIZE];
        let (status, target, len) = self.fetch_jspr(JSPRMethod::GET, JSPRTarget::SimInterface, "{}", &mut message).await.ok()?;

        if status != JSPRResultCode::Ok || target != JSPRTarget::SimInterface {
            if self.debug { info!("[ROCK] Failed to get SIM interface, status code: {}", status.as_str()); }
            return None;
        }

        let (response, _) = serde_json_core::from_slice::<JSPRGetSimInterface>(&message[..len]).ok()?;
        Some(response)
    }

    pub async fn put_sim_interface(&mut self, interface: &str) -> (Option<JSPRGetSimStatus>, Option<JSPRGetSimInterface>) {
        // send_jspr("PUT simInterface {\"interface\":\"internal\"}\r")
        let body = format!(64; "{{\"interface\":\"{}\"}}", interface).expect("Failed to format body");
        let mut message: [u8; RESPONSE_SIZE] = [0; RESPONSE_SIZE];
        let (status, target, len) = match self.fetch_jspr(JSPRMethod::PUT, JSPRTarget::SimInterface, &body, &mut message).await.ok() {
            Some(v) => v,
            None => return (None, None),
        };

        match (status, target) {
            (JSPRResultCode::UnsolicitedMessage, JSPRTarget::SimStatus) => {
                let (response, _) = match serde_json_core::from_slice::<JSPRGetSimStatus>(&message[..len]).ok() {
                    Some(v) => v,
                    None => return (None, None),
                };
                (Some(response), None)
            }
            (JSPRResultCode::Ok, JSPRTarget::SimInterface) => {
                let (response, _) = match serde_json_core::from_slice::<JSPRGetSimInterface>(&message[..len]).ok() {
                    Some(v) => v,
                    None => return (None, None),
                };
                (None, Some(response))
            }
            (other) => {
                if self.debug { info!("[ROCK] Failed to put SIM interface, status code: {}", other.0.as_str()); }
                (None, None)
            }
        }
    }

    pub async fn get_operational_state(&mut self) -> Option<JSPRGetOperationalState> {
        // send_jspr("GET operationalState {}\r")
        let mut message: [u8; RESPONSE_SIZE] = [0; RESPONSE_SIZE];
        let (status, target, len) = self.fetch_jspr(JSPRMethod::GET, JSPRTarget::OperationalState, "{}", &mut message).await.ok()?;

        if status != JSPRResultCode::Ok || target != JSPRTarget::OperationalState {
            if self.debug { info!("[ROCK] Failed to get Operational State, status code: {}", status.as_str()); }
            return None;
        }

        let (response, _) = serde_json_core::from_slice::<JSPRGetOperationalState>(&message[..len]).ok()?;
        Some(response)
    }

    pub async fn put_operational_state(&mut self, state: &str) -> Option<JSPRGetOperationalState> {
        // send_jspr("PUT operationalState {\"state\":\"active\"}\r")
        let body = format!(64; "{{\"state\":\"{}\"}}", state).expect("Failed to format body");
        let mut message: [u8; RESPONSE_SIZE] = [0; RESPONSE_SIZE];
        let (status, target, len) = self.fetch_jspr(JSPRMethod::PUT, JSPRTarget::OperationalState, &body, &mut message).await.ok()?;

        if status != JSPRResultCode::Ok || target != JSPRTarget::OperationalState {
            if self.debug { info!("[ROCK] Failed to put Operational State, status code: {}", status.as_str()); }
            return None;
        }

        let (response, _) = serde_json_core::from_slice::<JSPRGetOperationalState>(&message[..len]).ok()?;
        Some(response)
    }

    pub async fn get_message_provisioning(&mut self) -> Option<JSPRGetMessageProvisioning> {
        // send_jspr("GET messageProvisioning {}\r")
        let mut message: [u8; 4096] = [0; 4096];
        let (status, target, len) = self.fetch_jspr(JSPRMethod::GET, JSPRTarget::MessageProvisioning, "{}", &mut message).await.ok()?;

        if status != JSPRResultCode::Ok || target != JSPRTarget::MessageProvisioning {
            if self.debug { info!("[ROCK] Failed to get Message Provisioning, status code: {}", status.as_str()); }
            return None;
        }

        let (response, _) = serde_json_core::from_slice::<JSPRGetMessageProvisioning>(&message[..len]).ok()?;
        Some(response)
    }

    pub async fn put_message_originate(&mut self, topic: u16, length: u8) -> Option<JSPRPutMessageOriginate> {
        let body = format!(128; "{{\"topic_id\":{},\"message_length\":{},\"request_reference\":{}}}", topic, length, self.message_reference + 1).expect("Failed to format body");
        self.message_reference = (self.message_reference + 1) % 100;
        let mut message: [u8; RESPONSE_SIZE] = [0; RESPONSE_SIZE];
        let (status, target, len) = self.fetch_jspr(JSPRMethod::PUT, JSPRTarget::MessageOriginate, &body, &mut message).await.ok()?;

        if status != JSPRResultCode::Ok || target != JSPRTarget::MessageOriginate {
            if self.debug { info!("[ROCK] Failed to put Message Originate, status code: {}", status.as_str()); }
            return None;
        }

        let (response, _) = serde_json_core::from_slice::<JSPRPutMessageOriginate>(&message[..len]).ok()?;
        Some(response)
    }

    pub async fn put_message_originate_segment(&mut self, topic: u16, message_id: u16, segment_start: u32, segment_length: u16, data: &str) -> Option<JSPRPutMessageOriginateSegment> {
        let body = format!({128*3}; "{{\"topic_id\":{},\"message_id\":{},\"segment_start\":{},\"segment_length\":{},\"data\":\"{}\"}}", topic, message_id, segment_start, segment_length, data).expect("Failed to format body");
        let mut message: [u8; RESPONSE_SIZE] = [0; RESPONSE_SIZE];
        let (status, target, len) = self.fetch_jspr(JSPRMethod::PUT, JSPRTarget::MessageOriginateSegment, &body, &mut message).await.ok()?;

        if status != JSPRResultCode::Ok || target != JSPRTarget::MessageOriginateSegment {
            if self.debug { info!("[ROCK] Failed to put Message Originate Segment, status code: {}", status.as_str()); }
            return None;
        }

        let (response, _) = serde_json_core::from_slice::<JSPRPutMessageOriginateSegment>(&message[..len]).ok()?;
        Some(response)
    }

    pub async fn get_constellation_state(&mut self) -> Option<JSPRGetConstellationState> {
        // send_jspr("GET constellationState {}\r")
        let mut message: [u8; RESPONSE_SIZE] = [0; RESPONSE_SIZE];
        let (status, target, len) = self.fetch_jspr(JSPRMethod::GET, JSPRTarget::ConstellationState, "{}", &mut message).await.ok()?;

        if status != JSPRResultCode::Ok || target != JSPRTarget::ConstellationState {
            if self.debug { info!("[ROCK] Failed to get Constellation State, status code: {}", status.as_str()); }
            return None;
        }

        let (response, _) = serde_json_core::from_slice::<JSPRGetConstellationState>(&message[..len]).ok()?;
        Some(response)
    }

    pub async fn get_hw_info(&mut self) -> Option<JSPRGetHwInfo> {
        // send_jspr("GET hwInfo {}\r")
        let mut message: [u8; RESPONSE_SIZE] = [0; RESPONSE_SIZE];
        let (status, target, len) = self.fetch_jspr(JSPRMethod::GET, JSPRTarget::HwInfo, "{}", &mut message).await.ok()?;
        
        if status != JSPRResultCode::Ok || target != JSPRTarget::HwInfo {
            if self.debug { info!("[ROCK] Failed to get Hardware Info, status code: {}", status.as_str()); }
            return None;
        }

        let (response, _) = serde_json_core::from_slice::<JSPRGetHwInfo>(&message[..len]).ok()?;
        Some(response)
    }

    pub async fn get_sim_status(&mut self) -> Option<JSPRGetSimStatus> {
        // send_jspr("GET simStatus {}\r")
        let mut message: [u8; RESPONSE_SIZE] = [0; RESPONSE_SIZE];
        let (status, target, len) = self.fetch_jspr(JSPRMethod::GET, JSPRTarget::SimStatus, "{}", &mut message).await.ok()?;

        if status != JSPRResultCode::Ok || target != JSPRTarget::SimStatus {
            if self.debug { info!("[ROCK] Failed to get SIM Status, status code: {}", status.as_str()); }
            return None;
        }

        let (response, _) = serde_json_core::from_slice::<JSPRGetSimStatus>(&message[..len]).ok()?;
        Some(response)
    }

    pub async fn fetch_jspr(
        &mut self, 
        method: JSPRMethod,
        target: JSPRTarget,
        body: &str,
        message: &mut [u8]
    ) -> Result<(JSPRResultCode, JSPRTarget, usize), RockBlockError> {
        self.send_jspr(method, target, body).await;
        self.receive_jspr(message, false).await
    }

    pub async fn send_jspr(
        &mut self, 
        method: JSPRMethod,
        target: JSPRTarget,
        body: &str
    ) {

        let request = format!({BODY_SIZE * 4}; "{} {} {}\r", method.as_str(), target.as_str(), body).expect("Failed to format request");
        if self.debug { info!("[ROCK] Sending: {}", request.trim_end()); }
        self.uart.write(request.as_str().as_bytes()).await.expect("Failed to write request");
    }

    pub async fn receive_jspr(
        &mut self, 
        message: &mut [u8],
        expect_unsolicited: bool
    ) -> Result<(JSPRResultCode, JSPRTarget, usize), RockBlockError> {
        let mut buffer: [u8; RESPONSE_SIZE] = [0; RESPONSE_SIZE];
        let mut index: usize = 0;
        info!("[ROCK] Waiting for response...");
        loop {
            let mut byte: [u8; 1] = [0; 1];
            match self.uart.read(&mut byte).await {
                Ok(_) => {
                    buffer[index] = byte[0];
                    if index >= buffer.len() {
                        return Err(RockBlockError::ReceiveOverflow);
                    }
                    if byte[0] == b'\r' {
                        break;
                    }
                    index += 1;
                },
                Err(e) => {
                    info!("[ROCK] Error reading UART: {}", e);
                    // break;
                }
            }
        }

        let mut response = core::str::from_utf8(&buffer[..index]).expect("Failed to parse response");
        let mut start_offset: usize = 0;

        if !expect_unsolicited && response.starts_with("299") {
            start_offset = response.find("200").map_or(0, |pos| pos);
            info!("[ROCK] Skipping unsolicited message, new start offset: {}", start_offset);
        }

        response = &response[start_offset..];

        let mut parts = response.splitn(3, ' ');

        let result_code_str = parts.next().ok_or(RockBlockError::InvalidResponse)?;
        let result_code = result_code_str.parse::<u16>().map_err(|_| RockBlockError::InvalidResponse)?;
        let result_code = JSPRResultCode::from_u16(result_code).ok_or(RockBlockError::InvalidResponse)?;
       
        let target_str = parts.next().ok_or(RockBlockError::InvalidResponse)?;
        let target = JSPRTarget::from_str(target_str).ok_or(RockBlockError::InvalidResponse)?;

        let body_str = parts.next().ok_or(RockBlockError::InvalidResponse)?;
        message[..body_str.len()].copy_from_slice(body_str.trim_end().as_bytes());
        if self.debug { info!("[ROCK] Parsed: {}, {}, {}", result_code.as_str(), target.as_str(), body_str); }

        Ok((result_code, target, body_str.len()))
    }
}