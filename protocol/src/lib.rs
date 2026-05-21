use std::convert::TryFrom;
use std::fmt;

const MAGIC: &[u8; 4] = b"WHY1";
const VERSION: u8 = 1;
const HEADER_LEN: usize = 28;
const DESCRIPTOR_SET_MAGIC: &[u8; 4] = b"WDES";
const DESCRIPTOR_SET_VERSION: u8 = 1;
const DESCRIPTOR_SIDECAR_HEADER_LEN: usize = 8;
const MOCK_BULK_MAGIC: &[u8; 4] = b"WBUL";
const MOCK_BULK_VERSION: u8 = 1;
const MOCK_BULK_HEADER_LEN: usize = 16;
pub const USBIP_HEADER_LEN: usize = 48;
const USBIP_NO_ISO_PACKETS: u32 = 0xffff_ffff;
const USB_DESC_DEVICE: u8 = 0x01;
const USB_DESC_CONFIGURATION: u8 = 0x02;
const USB_DESC_STRING: u8 = 0x03;
const USB_DESC_INTERFACE: u8 = 0x04;
const USB_DESC_ENDPOINT: u8 = 0x05;
const USB_DESC_HID_REPORT: u8 = 0x22;
const USB_REQ_SET_ADDRESS: u8 = 0x05;
const USB_REQ_GET_DESCRIPTOR: u8 = 0x06;
const USB_REQ_SET_CONFIGURATION: u8 = 0x09;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum FrameType {
    Request = 1,
    Response = 2,
    Event = 3,
    AttachRequest = 4,
    AttachResponse = 5,
    DetachRequest = 6,
    DetachResponse = 7,
    CancelRequest = 8,
    CancelResponse = 9,
    ResetRequest = 10,
    ResetResponse = 11,
}

impl TryFrom<u8> for FrameType {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Request),
            2 => Ok(Self::Response),
            3 => Ok(Self::Event),
            4 => Ok(Self::AttachRequest),
            5 => Ok(Self::AttachResponse),
            6 => Ok(Self::DetachRequest),
            7 => Ok(Self::DetachResponse),
            8 => Ok(Self::CancelRequest),
            9 => Ok(Self::CancelResponse),
            10 => Ok(Self::ResetRequest),
            11 => Ok(Self::ResetResponse),
            other => Err(ProtocolError::InvalidFrameType(other)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum TransferStatus {
    Ok = 0,
    Failed = -1,
    Cancelled = -2,
    Timeout = -3,
    Stall = -4,
    Reset = -5,
}

impl TryFrom<i32> for TransferStatus {
    type Error = ProtocolError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Ok),
            -1 => Ok(Self::Failed),
            -2 => Ok(Self::Cancelled),
            -3 => Ok(Self::Timeout),
            -4 => Ok(Self::Stall),
            -5 => Ok(Self::Reset),
            other => Err(ProtocolError::InvalidTransferStatus(other)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Direction {
    HostToDevice = 1,
    DeviceToHost = 2,
}

impl TryFrom<u8> for Direction {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::HostToDevice),
            2 => Ok(Self::DeviceToHost),
            other => Err(ProtocolError::InvalidDirection(other)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum TransferType {
    Control = 1,
    Bulk = 2,
    Interrupt = 3,
    Isochronous = 4,
}

impl TryFrom<u8> for TransferType {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Control),
            2 => Ok(Self::Bulk),
            3 => Ok(Self::Interrupt),
            4 => Ok(Self::Isochronous),
            other => Err(ProtocolError::InvalidTransferType(other)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum UsbIpCommand {
    CmdSubmit = 0x0000_0001,
    CmdUnlink = 0x0000_0002,
    RetSubmit = 0x0000_0003,
    RetUnlink = 0x0000_0004,
}

impl TryFrom<u32> for UsbIpCommand {
    type Error = ProtocolError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0x0000_0001 => Ok(Self::CmdSubmit),
            0x0000_0002 => Ok(Self::CmdUnlink),
            0x0000_0003 => Ok(Self::RetSubmit),
            0x0000_0004 => Ok(Self::RetUnlink),
            _ => Err(ProtocolError::InvalidUsbIpPacket("unsupported command")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum UsbIpDirection {
    Out = 0,
    In = 1,
}

impl TryFrom<u32> for UsbIpDirection {
    type Error = ProtocolError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Out),
            1 => Ok(Self::In),
            _ => Err(ProtocolError::InvalidUsbIpPacket("unsupported direction")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsbIpHeaderBasic {
    pub command: UsbIpCommand,
    pub seqnum: u32,
    pub devid: u32,
    pub direction: UsbIpDirection,
    pub endpoint: u32,
}

impl UsbIpHeaderBasic {
    pub const LEN: usize = 20;

    pub fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&(self.command as u32).to_be_bytes());
        out.extend_from_slice(&self.seqnum.to_be_bytes());
        out.extend_from_slice(&self.devid.to_be_bytes());
        out.extend_from_slice(&(self.direction as u32).to_be_bytes());
        out.extend_from_slice(&self.endpoint.to_be_bytes());
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() < Self::LEN {
            return Err(ProtocolError::InvalidUsbIpPacket("basic header too short"));
        }

        Ok(Self {
            command: UsbIpCommand::try_from(read_u32_be(bytes, 0))?,
            seqnum: read_u32_be(bytes, 4),
            devid: read_u32_be(bytes, 8),
            direction: UsbIpDirection::try_from(read_u32_be(bytes, 12))?,
            endpoint: read_u32_be(bytes, 16),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbIpCmdSubmit {
    pub header: UsbIpHeaderBasic,
    pub transfer_flags: u32,
    pub transfer_buffer_length: u32,
    pub start_frame: u32,
    pub number_of_packets: u32,
    pub interval: u32,
    pub setup: [u8; 8],
    pub transfer_buffer: Vec<u8>,
}

impl UsbIpCmdSubmit {
    pub fn new(
        seqnum: u32,
        devid: u32,
        direction: UsbIpDirection,
        endpoint: u32,
        transfer_buffer: Vec<u8>,
        setup: [u8; 8],
    ) -> Self {
        Self {
            header: UsbIpHeaderBasic {
                command: UsbIpCommand::CmdSubmit,
                seqnum,
                devid,
                direction,
                endpoint,
            },
            transfer_flags: 0,
            transfer_buffer_length: transfer_buffer.len() as u32,
            start_frame: 0,
            number_of_packets: USBIP_NO_ISO_PACKETS,
            interval: 0,
            setup,
            transfer_buffer,
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        if self.header.command != UsbIpCommand::CmdSubmit {
            return Err(ProtocolError::InvalidUsbIpPacket(
                "submit header command mismatch",
            ));
        }
        if self.header.direction == UsbIpDirection::Out
            && self.transfer_buffer.len() != self.transfer_buffer_length as usize
        {
            return Err(ProtocolError::InvalidUsbIpPacket(
                "OUT transfer length mismatch",
            ));
        }
        if self.header.direction == UsbIpDirection::In && !self.transfer_buffer.is_empty() {
            return Err(ProtocolError::InvalidUsbIpPacket(
                "IN submit must not carry a transfer buffer",
            ));
        }

        let mut out = Vec::with_capacity(USBIP_HEADER_LEN + self.transfer_buffer.len());
        self.header.encode(&mut out);
        out.extend_from_slice(&self.transfer_flags.to_be_bytes());
        out.extend_from_slice(&self.transfer_buffer_length.to_be_bytes());
        out.extend_from_slice(&self.start_frame.to_be_bytes());
        out.extend_from_slice(&self.number_of_packets.to_be_bytes());
        out.extend_from_slice(&self.interval.to_be_bytes());
        out.extend_from_slice(&self.setup);
        out.extend_from_slice(&self.transfer_buffer);
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() < USBIP_HEADER_LEN {
            return Err(ProtocolError::InvalidUsbIpPacket("submit too short"));
        }

        let header = UsbIpHeaderBasic::decode(&bytes[..UsbIpHeaderBasic::LEN])?;
        if header.command != UsbIpCommand::CmdSubmit {
            return Err(ProtocolError::InvalidUsbIpPacket("submit command mismatch"));
        }
        let transfer_buffer_length = read_u32_be(bytes, 24);
        let expected_payload_len = if header.direction == UsbIpDirection::Out {
            transfer_buffer_length as usize
        } else {
            0
        };
        if bytes.len() != USBIP_HEADER_LEN + expected_payload_len {
            return Err(ProtocolError::InvalidUsbIpPacket(
                "submit payload length mismatch",
            ));
        }

        Ok(Self {
            header,
            transfer_flags: read_u32_be(bytes, 20),
            transfer_buffer_length,
            start_frame: read_u32_be(bytes, 28),
            number_of_packets: read_u32_be(bytes, 32),
            interval: read_u32_be(bytes, 36),
            setup: bytes[40..48].try_into().unwrap(),
            transfer_buffer: bytes[USBIP_HEADER_LEN..].to_vec(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbIpRetSubmit {
    pub header: UsbIpHeaderBasic,
    pub status: i32,
    pub actual_length: u32,
    pub start_frame: u32,
    pub number_of_packets: u32,
    pub error_count: u32,
    pub transfer_buffer: Vec<u8>,
}

impl UsbIpRetSubmit {
    pub fn ok_for(submit: &UsbIpCmdSubmit, transfer_buffer: Vec<u8>) -> Self {
        Self {
            header: UsbIpHeaderBasic {
                command: UsbIpCommand::RetSubmit,
                seqnum: submit.header.seqnum,
                devid: 0,
                direction: UsbIpDirection::Out,
                endpoint: 0,
            },
            status: 0,
            actual_length: transfer_buffer.len() as u32,
            start_frame: 0,
            number_of_packets: USBIP_NO_ISO_PACKETS,
            error_count: 0,
            transfer_buffer,
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        if self.header.command != UsbIpCommand::RetSubmit {
            return Err(ProtocolError::InvalidUsbIpPacket(
                "ret submit header command mismatch",
            ));
        }
        if self.transfer_buffer.len() != self.actual_length as usize {
            return Err(ProtocolError::InvalidUsbIpPacket(
                "ret submit actual length mismatch",
            ));
        }

        let mut out = Vec::with_capacity(USBIP_HEADER_LEN + self.transfer_buffer.len());
        self.header.encode(&mut out);
        out.extend_from_slice(&self.status.to_be_bytes());
        out.extend_from_slice(&self.actual_length.to_be_bytes());
        out.extend_from_slice(&self.start_frame.to_be_bytes());
        out.extend_from_slice(&self.number_of_packets.to_be_bytes());
        out.extend_from_slice(&self.error_count.to_be_bytes());
        out.extend_from_slice(&[0; 8]);
        out.extend_from_slice(&self.transfer_buffer);
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() < USBIP_HEADER_LEN {
            return Err(ProtocolError::InvalidUsbIpPacket("ret submit too short"));
        }

        let header = UsbIpHeaderBasic::decode(&bytes[..UsbIpHeaderBasic::LEN])?;
        if header.command != UsbIpCommand::RetSubmit {
            return Err(ProtocolError::InvalidUsbIpPacket(
                "ret submit command mismatch",
            ));
        }
        let actual_length = read_u32_be(bytes, 24);
        if bytes.len() != USBIP_HEADER_LEN + actual_length as usize {
            return Err(ProtocolError::InvalidUsbIpPacket(
                "ret submit payload length mismatch",
            ));
        }

        Ok(Self {
            header,
            status: i32::from_be_bytes(bytes[20..24].try_into().unwrap()),
            actual_length,
            start_frame: read_u32_be(bytes, 28),
            number_of_packets: read_u32_be(bytes, 32),
            error_count: read_u32_be(bytes, 36),
            transfer_buffer: bytes[USBIP_HEADER_LEN..].to_vec(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbIpCmdUnlink {
    pub header: UsbIpHeaderBasic,
    pub unlink_seqnum: u32,
}

impl UsbIpCmdUnlink {
    pub fn new(seqnum: u32, devid: u32, unlink_seqnum: u32) -> Self {
        Self {
            header: UsbIpHeaderBasic {
                command: UsbIpCommand::CmdUnlink,
                seqnum,
                devid,
                direction: UsbIpDirection::Out,
                endpoint: 0,
            },
            unlink_seqnum,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(USBIP_HEADER_LEN);
        self.header.encode(&mut out);
        out.extend_from_slice(&self.unlink_seqnum.to_be_bytes());
        out.extend_from_slice(&[0; 24]);
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() != USBIP_HEADER_LEN {
            return Err(ProtocolError::InvalidUsbIpPacket("unlink length mismatch"));
        }

        let header = UsbIpHeaderBasic::decode(&bytes[..UsbIpHeaderBasic::LEN])?;
        if header.command != UsbIpCommand::CmdUnlink {
            return Err(ProtocolError::InvalidUsbIpPacket("unlink command mismatch"));
        }

        Ok(Self {
            header,
            unlink_seqnum: read_u32_be(bytes, 20),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbIpRetUnlink {
    pub header: UsbIpHeaderBasic,
    pub status: i32,
}

impl UsbIpRetUnlink {
    pub fn new(seqnum: u32, status: i32) -> Self {
        Self {
            header: UsbIpHeaderBasic {
                command: UsbIpCommand::RetUnlink,
                seqnum,
                devid: 0,
                direction: UsbIpDirection::Out,
                endpoint: 0,
            },
            status,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(USBIP_HEADER_LEN);
        self.header.encode(&mut out);
        out.extend_from_slice(&self.status.to_be_bytes());
        out.extend_from_slice(&[0; 24]);
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() != USBIP_HEADER_LEN {
            return Err(ProtocolError::InvalidUsbIpPacket(
                "ret unlink length mismatch",
            ));
        }

        let header = UsbIpHeaderBasic::decode(&bytes[..UsbIpHeaderBasic::LEN])?;
        if header.command != UsbIpCommand::RetUnlink {
            return Err(ProtocolError::InvalidUsbIpPacket(
                "ret unlink command mismatch",
            ));
        }

        Ok(Self {
            header,
            status: i32::from_be_bytes(bytes[20..24].try_into().unwrap()),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum UsbDescriptorType {
    Device = USB_DESC_DEVICE,
    Configuration = USB_DESC_CONFIGURATION,
    String = USB_DESC_STRING,
    Interface = USB_DESC_INTERFACE,
    Endpoint = USB_DESC_ENDPOINT,
    Report = USB_DESC_HID_REPORT,
}

impl TryFrom<u8> for UsbDescriptorType {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            USB_DESC_DEVICE => Ok(Self::Device),
            USB_DESC_CONFIGURATION => Ok(Self::Configuration),
            USB_DESC_STRING => Ok(Self::String),
            USB_DESC_INTERFACE => Ok(Self::Interface),
            USB_DESC_ENDPOINT => Ok(Self::Endpoint),
            USB_DESC_HID_REPORT => Ok(Self::Report),
            _ => Err(ProtocolError::InvalidControlTransfer(
                "unsupported descriptor type",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbStandardRequest {
    SetAddress,
    GetDescriptor,
    SetConfiguration,
}

impl TryFrom<u8> for UsbStandardRequest {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            USB_REQ_SET_ADDRESS => Ok(Self::SetAddress),
            USB_REQ_GET_DESCRIPTOR => Ok(Self::GetDescriptor),
            USB_REQ_SET_CONFIGURATION => Ok(Self::SetConfiguration),
            _ => Err(ProtocolError::InvalidControlTransfer(
                "unsupported standard request",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsbControlSetup {
    pub request_type: u8,
    pub request: u8,
    pub value: u16,
    pub index: u16,
    pub length: u16,
}

impl UsbControlSetup {
    pub const LEN: usize = 8;

    pub fn get_descriptor(
        descriptor_type: UsbDescriptorType,
        descriptor_index: u8,
        length: u16,
    ) -> Self {
        Self {
            request_type: 0x80,
            request: USB_REQ_GET_DESCRIPTOR,
            value: ((descriptor_type as u16) << 8) | u16::from(descriptor_index),
            index: 0,
            length,
        }
    }

    pub fn get_interface_descriptor(
        descriptor_type: UsbDescriptorType,
        descriptor_index: u8,
        interface_number: u8,
        length: u16,
    ) -> Self {
        Self {
            request_type: 0x81,
            request: USB_REQ_GET_DESCRIPTOR,
            value: ((descriptor_type as u16) << 8) | u16::from(descriptor_index),
            index: u16::from(interface_number),
            length,
        }
    }

    pub fn set_address(address: u8) -> Self {
        Self {
            request_type: 0x00,
            request: USB_REQ_SET_ADDRESS,
            value: u16::from(address),
            index: 0,
            length: 0,
        }
    }

    pub fn set_configuration(configuration_value: u8) -> Self {
        Self {
            request_type: 0x00,
            request: USB_REQ_SET_CONFIGURATION,
            value: u16::from(configuration_value),
            index: 0,
            length: 0,
        }
    }

    pub fn standard_request(&self) -> Result<UsbStandardRequest, ProtocolError> {
        UsbStandardRequest::try_from(self.request)
    }

    pub fn descriptor_type(&self) -> Result<UsbDescriptorType, ProtocolError> {
        UsbDescriptorType::try_from((self.value >> 8) as u8)
    }

    pub fn descriptor_index(&self) -> u8 {
        (self.value & 0xff) as u8
    }

    pub fn transfer_direction(&self) -> Direction {
        if self.request_type & 0x80 != 0 {
            Direction::DeviceToHost
        } else {
            Direction::HostToDevice
        }
    }

    pub fn encode(&self) -> [u8; Self::LEN] {
        let mut out = [0u8; Self::LEN];

        out[0] = self.request_type;
        out[1] = self.request;
        out[2..4].copy_from_slice(&self.value.to_le_bytes());
        out[4..6].copy_from_slice(&self.index.to_le_bytes());
        out[6..8].copy_from_slice(&self.length.to_le_bytes());
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() != Self::LEN {
            return Err(ProtocolError::InvalidControlTransfer(
                "setup packet must be 8 bytes",
            ));
        }

        Ok(Self {
            request_type: bytes[0],
            request: bytes[1],
            value: read_u16_le(bytes, 2),
            index: read_u16_le(bytes, 4),
            length: read_u16_le(bytes, 6),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbControlTransfer {
    pub setup: UsbControlSetup,
    pub data: Vec<u8>,
}

impl UsbControlTransfer {
    pub fn new(setup: UsbControlSetup, data: Vec<u8>) -> Self {
        Self { setup, data }
    }

    pub fn setup_only(setup: UsbControlSetup) -> Self {
        Self::new(setup, Vec::new())
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(UsbControlSetup::LEN + self.data.len());

        out.extend_from_slice(&self.setup.encode());
        out.extend_from_slice(&self.data);
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() < UsbControlSetup::LEN {
            return Err(ProtocolError::InvalidControlTransfer(
                "control transfer payload too short",
            ));
        }

        Ok(Self {
            setup: UsbControlSetup::decode(&bytes[..UsbControlSetup::LEN])?,
            data: bytes[UsbControlSetup::LEN..].to_vec(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockBulkPayload {
    pub data: Vec<u8>,
}

impl MockBulkPayload {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    pub fn synthetic(request_id: u64, data_len: usize) -> Self {
        let mut data = Vec::with_capacity(data_len);
        let seed = request_id.to_le_bytes();

        for index in 0..data_len {
            data.push(seed[index % seed.len()].wrapping_add(index as u8));
        }

        Self { data }
    }

    pub fn checksum(&self) -> u32 {
        checksum(&self.data)
    }

    pub fn encode(&self) -> Vec<u8> {
        let data_len = u32::try_from(self.data.len()).expect("mock bulk payload too large");
        let mut out = Vec::with_capacity(MOCK_BULK_HEADER_LEN + self.data.len());

        out.extend_from_slice(MOCK_BULK_MAGIC);
        out.push(MOCK_BULK_VERSION);
        out.extend_from_slice(&[0, 0, 0]);
        out.extend_from_slice(&data_len.to_be_bytes());
        out.extend_from_slice(&self.checksum().to_be_bytes());
        out.extend_from_slice(&self.data);
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() < MOCK_BULK_HEADER_LEN {
            return Err(ProtocolError::InvalidBulkPayload("payload too short"));
        }

        if &bytes[0..4] != MOCK_BULK_MAGIC {
            return Err(ProtocolError::InvalidBulkPayload("invalid magic"));
        }

        if bytes[4] != MOCK_BULK_VERSION {
            return Err(ProtocolError::InvalidBulkPayload("unsupported version"));
        }

        let data_len = u32::from_be_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let expected_checksum = u32::from_be_bytes(bytes[12..16].try_into().unwrap());
        let expected_len = MOCK_BULK_HEADER_LEN + data_len;
        if bytes.len() != expected_len {
            return Err(ProtocolError::InvalidBulkPayload("data length mismatch"));
        }

        let data = bytes[MOCK_BULK_HEADER_LEN..].to_vec();
        if checksum(&data) != expected_checksum {
            return Err(ProtocolError::InvalidBulkPayload("checksum mismatch"));
        }

        Ok(Self { data })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HidKeyboardInputReport {
    pub modifiers: u8,
    pub reserved: u8,
    pub keycodes: [u8; 6],
}

impl HidKeyboardInputReport {
    pub const LEN: usize = 8;
    pub const KEY_A: u8 = 0x04;

    pub fn empty() -> Self {
        Self {
            modifiers: 0,
            reserved: 0,
            keycodes: [0; 6],
        }
    }

    pub fn key_press(keycode: u8) -> Self {
        let mut report = Self::empty();
        report.keycodes[0] = keycode;
        report
    }

    pub fn encode(&self) -> [u8; Self::LEN] {
        [
            self.modifiers,
            self.reserved,
            self.keycodes[0],
            self.keycodes[1],
            self.keycodes[2],
            self.keycodes[3],
            self.keycodes[4],
            self.keycodes[5],
        ]
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() != Self::LEN {
            return Err(ProtocolError::InvalidHidReport(
                "keyboard input report must be 8 bytes",
            ));
        }

        Ok(Self {
            modifiers: bytes[0],
            reserved: bytes[1],
            keycodes: [bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]],
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbDescriptorSet {
    pub device: UsbDeviceDescriptor,
    pub configurations: Vec<UsbConfiguration>,
}

impl UsbDescriptorSet {
    pub fn mock_hid_keyboard(vendor_id: u16, product_id: u16) -> Self {
        let report_descriptor = hid_keyboard_report_descriptor();
        Self {
            device: UsbDeviceDescriptor {
                usb_version: 0x0200,
                device_class: 0,
                device_sub_class: 0,
                device_protocol: 0,
                max_packet_size_0: 64,
                vendor_id,
                product_id,
                device_version: 0x0100,
                manufacturer_index: 1,
                product_index: 2,
                serial_number_index: 3,
            },
            configurations: vec![UsbConfiguration {
                descriptor: UsbConfigurationDescriptor {
                    configuration_value: 1,
                    configuration_index: 0,
                    attributes: 0x80,
                    max_power: 50,
                },
                extra_descriptors: Vec::new(),
                interfaces: vec![UsbInterface {
                    descriptor: UsbInterfaceDescriptor {
                        interface_number: 0,
                        alternate_setting: 0,
                        interface_class: 0x03,
                        interface_sub_class: 0x01,
                        interface_protocol: 0x01,
                        interface_index: 0,
                    },
                    extra_descriptors: vec![vec![
                        0x09, 0x21, 0x11, 0x01, 0x00, 0x01, 0x22, 0x3f, 0x00,
                    ]],
                    report_descriptor: Some(report_descriptor),
                    endpoints: vec![UsbEndpointDescriptor {
                        address: 0x81,
                        attributes: 0x03,
                        max_packet_size: 8,
                        interval: 10,
                    }],
                }],
            }],
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        let config_count = u8::try_from(self.configurations.len())
            .map_err(|_| ProtocolError::InvalidDescriptorSet("too many configurations"))?;
        let sidecars = self.report_descriptor_sidecars()?;
        let sidecar_count = u16::try_from(sidecars.len())
            .map_err(|_| ProtocolError::InvalidDescriptorSet("too many sidecar descriptors"))?;
        let mut out = Vec::new();

        out.extend_from_slice(DESCRIPTOR_SET_MAGIC);
        out.push(DESCRIPTOR_SET_VERSION);
        out.push(config_count);
        out.extend_from_slice(&sidecar_count.to_le_bytes());
        out.extend_from_slice(&self.device.to_usb_bytes(config_count));

        for configuration in &self.configurations {
            out.extend_from_slice(&configuration.to_usb_bytes()?);
        }

        for sidecar in sidecars {
            out.extend_from_slice(&sidecar);
        }

        Ok(out)
    }

    pub fn device_descriptor_bytes(&self) -> Result<Vec<u8>, ProtocolError> {
        let config_count = u8::try_from(self.configurations.len())
            .map_err(|_| ProtocolError::InvalidDescriptorSet("too many configurations"))?;

        Ok(self.device.to_usb_bytes(config_count).to_vec())
    }

    pub fn configuration_descriptor_bytes(
        &self,
        descriptor_index: u8,
    ) -> Result<Vec<u8>, ProtocolError> {
        self.configurations
            .get(descriptor_index as usize)
            .ok_or(ProtocolError::InvalidDescriptorSet(
                "configuration descriptor index out of range",
            ))?
            .to_usb_bytes()
    }

    pub fn report_descriptor_bytes(
        &self,
        configuration_index: u8,
        interface_number: u8,
        descriptor_index: u8,
    ) -> Result<Vec<u8>, ProtocolError> {
        if descriptor_index != 0 {
            return Err(ProtocolError::InvalidDescriptorSet(
                "report descriptor index out of range",
            ));
        }

        let configuration = self
            .configurations
            .get(configuration_index as usize)
            .ok_or(ProtocolError::InvalidDescriptorSet(
                "configuration descriptor index out of range",
            ))?;
        let interface = configuration
            .interfaces
            .iter()
            .find(|interface| interface.descriptor.interface_number == interface_number)
            .ok_or(ProtocolError::InvalidDescriptorSet(
                "interface descriptor index out of range",
            ))?;

        interface
            .report_descriptor
            .clone()
            .ok_or(ProtocolError::InvalidDescriptorSet(
                "missing report descriptor",
            ))
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() < 8 + UsbDeviceDescriptor::LEN {
            return Err(ProtocolError::InvalidDescriptorSet("payload too short"));
        }

        if &bytes[0..4] != DESCRIPTOR_SET_MAGIC {
            return Err(ProtocolError::InvalidDescriptorSet(
                "invalid descriptor set magic",
            ));
        }

        if bytes[4] != DESCRIPTOR_SET_VERSION {
            return Err(ProtocolError::InvalidDescriptorSet(
                "unsupported descriptor set version",
            ));
        }

        let config_count = bytes[5] as usize;
        let sidecar_count = read_u16_le(bytes, 6) as usize;
        let mut offset = 8usize;
        let (device, device_config_count) = UsbDeviceDescriptor::from_usb_bytes(take(
            bytes,
            &mut offset,
            UsbDeviceDescriptor::LEN,
        )?)?;

        if device_config_count as usize != config_count {
            return Err(ProtocolError::InvalidDescriptorSet(
                "device configuration count mismatch",
            ));
        }

        let mut configurations = Vec::with_capacity(config_count);
        for _ in 0..config_count {
            configurations.push(UsbConfiguration::decode_from(bytes, &mut offset)?);
        }

        for _ in 0..sidecar_count {
            let header = take(bytes, &mut offset, DESCRIPTOR_SIDECAR_HEADER_LEN)?;
            let config_index = header[0] as usize;
            let interface_number = header[1];
            let alternate_setting = header[2];
            let descriptor_type = header[3];
            let descriptor_index = header[4];
            let descriptor_len = read_u16_le(header, 6) as usize;
            let descriptor = take(bytes, &mut offset, descriptor_len)?.to_vec();

            if descriptor_type != USB_DESC_HID_REPORT || descriptor_index != 0 {
                return Err(ProtocolError::InvalidDescriptorSet(
                    "unsupported sidecar descriptor",
                ));
            }

            let configuration =
                configurations
                    .get_mut(config_index)
                    .ok_or(ProtocolError::InvalidDescriptorSet(
                        "sidecar configuration index out of range",
                    ))?;
            let interface = configuration
                .interfaces
                .iter_mut()
                .find(|interface| {
                    interface.descriptor.interface_number == interface_number
                        && interface.descriptor.alternate_setting == alternate_setting
                })
                .ok_or(ProtocolError::InvalidDescriptorSet(
                    "sidecar interface index out of range",
                ))?;
            interface.report_descriptor = Some(descriptor);
        }

        if offset != bytes.len() {
            return Err(ProtocolError::InvalidDescriptorSet(
                "trailing descriptor payload bytes",
            ));
        }

        Ok(Self {
            device,
            configurations,
        })
    }

    fn report_descriptor_sidecars(&self) -> Result<Vec<Vec<u8>>, ProtocolError> {
        let mut out = Vec::new();

        for (config_index, configuration) in self.configurations.iter().enumerate() {
            let config_index = u8::try_from(config_index)
                .map_err(|_| ProtocolError::InvalidDescriptorSet("too many configurations"))?;
            for interface in &configuration.interfaces {
                let Some(report_descriptor) = &interface.report_descriptor else {
                    continue;
                };
                let descriptor_len = u16::try_from(report_descriptor.len()).map_err(|_| {
                    ProtocolError::InvalidDescriptorSet("report descriptor too large")
                })?;
                let mut sidecar =
                    Vec::with_capacity(DESCRIPTOR_SIDECAR_HEADER_LEN + report_descriptor.len());

                sidecar.push(config_index);
                sidecar.push(interface.descriptor.interface_number);
                sidecar.push(interface.descriptor.alternate_setting);
                sidecar.push(USB_DESC_HID_REPORT);
                sidecar.push(0);
                sidecar.push(0);
                sidecar.extend_from_slice(&descriptor_len.to_le_bytes());
                sidecar.extend_from_slice(report_descriptor);
                out.push(sidecar);
            }
        }

        Ok(out)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbDeviceDescriptor {
    pub usb_version: u16,
    pub device_class: u8,
    pub device_sub_class: u8,
    pub device_protocol: u8,
    pub max_packet_size_0: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_version: u16,
    pub manufacturer_index: u8,
    pub product_index: u8,
    pub serial_number_index: u8,
}

impl UsbDeviceDescriptor {
    const LEN: usize = 18;

    fn to_usb_bytes(&self, configuration_count: u8) -> [u8; Self::LEN] {
        let mut out = [0u8; Self::LEN];

        out[0] = Self::LEN as u8;
        out[1] = USB_DESC_DEVICE;
        out[2..4].copy_from_slice(&self.usb_version.to_le_bytes());
        out[4] = self.device_class;
        out[5] = self.device_sub_class;
        out[6] = self.device_protocol;
        out[7] = self.max_packet_size_0;
        out[8..10].copy_from_slice(&self.vendor_id.to_le_bytes());
        out[10..12].copy_from_slice(&self.product_id.to_le_bytes());
        out[12..14].copy_from_slice(&self.device_version.to_le_bytes());
        out[14] = self.manufacturer_index;
        out[15] = self.product_index;
        out[16] = self.serial_number_index;
        out[17] = configuration_count;
        out
    }

    fn from_usb_bytes(bytes: &[u8]) -> Result<(Self, u8), ProtocolError> {
        if bytes.len() != Self::LEN || bytes[0] != Self::LEN as u8 || bytes[1] != USB_DESC_DEVICE {
            return Err(ProtocolError::InvalidDescriptorSet(
                "invalid device descriptor",
            ));
        }

        Ok((
            Self {
                usb_version: read_u16_le(bytes, 2),
                device_class: bytes[4],
                device_sub_class: bytes[5],
                device_protocol: bytes[6],
                max_packet_size_0: bytes[7],
                vendor_id: read_u16_le(bytes, 8),
                product_id: read_u16_le(bytes, 10),
                device_version: read_u16_le(bytes, 12),
                manufacturer_index: bytes[14],
                product_index: bytes[15],
                serial_number_index: bytes[16],
            },
            bytes[17],
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbConfiguration {
    pub descriptor: UsbConfigurationDescriptor,
    pub extra_descriptors: Vec<Vec<u8>>,
    pub interfaces: Vec<UsbInterface>,
}

impl UsbConfiguration {
    fn to_usb_bytes(&self) -> Result<Vec<u8>, ProtocolError> {
        let interface_count = u8::try_from(self.interfaces.len())
            .map_err(|_| ProtocolError::InvalidDescriptorSet("too many interfaces"))?;
        let mut body = Vec::new();

        for extra in &self.extra_descriptors {
            validate_extra_descriptor(extra)?;
            body.extend_from_slice(extra);
        }

        for interface in &self.interfaces {
            body.extend_from_slice(&interface.to_usb_bytes()?);
        }

        let total_length = u16::try_from(UsbConfigurationDescriptor::LEN + body.len())
            .map_err(|_| ProtocolError::InvalidDescriptorSet("configuration too large"))?;
        let mut out = self.descriptor.to_usb_bytes(total_length, interface_count);
        out.extend_from_slice(&body);
        Ok(out)
    }

    fn decode_from(bytes: &[u8], offset: &mut usize) -> Result<Self, ProtocolError> {
        let config_bytes = take(bytes, offset, UsbConfigurationDescriptor::LEN)?;
        let (descriptor, total_length, interface_count) =
            UsbConfigurationDescriptor::from_usb_bytes(config_bytes)?;
        let body_len = usize::from(total_length)
            .checked_sub(UsbConfigurationDescriptor::LEN)
            .ok_or(ProtocolError::InvalidDescriptorSet(
                "invalid configuration total length",
            ))?;
        let body = take(bytes, offset, body_len)?;
        let mut body_offset = 0usize;
        let mut extra_descriptors = Vec::new();
        let mut interfaces = Vec::new();
        let mut current: Option<OpenInterface> = None;

        while body_offset < body.len() {
            let (descriptor_len, descriptor_type) = descriptor_header(body, body_offset)?;
            let descriptor_bytes = take(body, &mut body_offset, descriptor_len)?;

            match descriptor_type {
                USB_DESC_INTERFACE => {
                    if let Some(open) = current.take() {
                        interfaces.push(open.finish()?);
                    }
                    let (descriptor, endpoint_count) =
                        UsbInterfaceDescriptor::from_usb_bytes(descriptor_bytes)?;
                    current = Some(OpenInterface {
                        descriptor,
                        expected_endpoints: endpoint_count,
                        extra_descriptors: Vec::new(),
                        endpoints: Vec::new(),
                    });
                }
                USB_DESC_ENDPOINT => {
                    let Some(open) = current.as_mut() else {
                        return Err(ProtocolError::InvalidDescriptorSet(
                            "endpoint descriptor before interface",
                        ));
                    };
                    open.endpoints
                        .push(UsbEndpointDescriptor::from_usb_bytes(descriptor_bytes)?);
                }
                _ if current.is_some() => {
                    validate_extra_descriptor(descriptor_bytes)?;
                    current
                        .as_mut()
                        .expect("checked above")
                        .extra_descriptors
                        .push(descriptor_bytes.to_vec());
                }
                _ => {
                    validate_extra_descriptor(descriptor_bytes)?;
                    extra_descriptors.push(descriptor_bytes.to_vec());
                }
            }
        }

        if let Some(open) = current.take() {
            interfaces.push(open.finish()?);
        }

        if interfaces.len() != interface_count as usize {
            return Err(ProtocolError::InvalidDescriptorSet(
                "interface count mismatch",
            ));
        }

        Ok(Self {
            descriptor,
            extra_descriptors,
            interfaces,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbConfigurationDescriptor {
    pub configuration_value: u8,
    pub configuration_index: u8,
    pub attributes: u8,
    pub max_power: u8,
}

impl UsbConfigurationDescriptor {
    const LEN: usize = 9;

    fn to_usb_bytes(&self, total_length: u16, interface_count: u8) -> Vec<u8> {
        let mut out = vec![0u8; Self::LEN];

        out[0] = Self::LEN as u8;
        out[1] = USB_DESC_CONFIGURATION;
        out[2..4].copy_from_slice(&total_length.to_le_bytes());
        out[4] = interface_count;
        out[5] = self.configuration_value;
        out[6] = self.configuration_index;
        out[7] = self.attributes;
        out[8] = self.max_power;
        out
    }

    fn from_usb_bytes(bytes: &[u8]) -> Result<(Self, u16, u8), ProtocolError> {
        if bytes.len() != Self::LEN
            || bytes[0] != Self::LEN as u8
            || bytes[1] != USB_DESC_CONFIGURATION
        {
            return Err(ProtocolError::InvalidDescriptorSet(
                "invalid configuration descriptor",
            ));
        }

        Ok((
            Self {
                configuration_value: bytes[5],
                configuration_index: bytes[6],
                attributes: bytes[7],
                max_power: bytes[8],
            },
            read_u16_le(bytes, 2),
            bytes[4],
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbInterface {
    pub descriptor: UsbInterfaceDescriptor,
    pub extra_descriptors: Vec<Vec<u8>>,
    pub report_descriptor: Option<Vec<u8>>,
    pub endpoints: Vec<UsbEndpointDescriptor>,
}

impl UsbInterface {
    fn to_usb_bytes(&self) -> Result<Vec<u8>, ProtocolError> {
        let endpoint_count = u8::try_from(self.endpoints.len())
            .map_err(|_| ProtocolError::InvalidDescriptorSet("too many endpoints"))?;
        let mut out = self.descriptor.to_usb_bytes(endpoint_count);

        for extra in &self.extra_descriptors {
            validate_extra_descriptor(extra)?;
            out.extend_from_slice(extra);
        }

        for endpoint in &self.endpoints {
            out.extend_from_slice(&endpoint.to_usb_bytes());
        }

        Ok(out)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbInterfaceDescriptor {
    pub interface_number: u8,
    pub alternate_setting: u8,
    pub interface_class: u8,
    pub interface_sub_class: u8,
    pub interface_protocol: u8,
    pub interface_index: u8,
}

impl UsbInterfaceDescriptor {
    const LEN: usize = 9;

    fn to_usb_bytes(&self, endpoint_count: u8) -> Vec<u8> {
        vec![
            Self::LEN as u8,
            USB_DESC_INTERFACE,
            self.interface_number,
            self.alternate_setting,
            endpoint_count,
            self.interface_class,
            self.interface_sub_class,
            self.interface_protocol,
            self.interface_index,
        ]
    }

    fn from_usb_bytes(bytes: &[u8]) -> Result<(Self, u8), ProtocolError> {
        if bytes.len() != Self::LEN || bytes[0] != Self::LEN as u8 || bytes[1] != USB_DESC_INTERFACE
        {
            return Err(ProtocolError::InvalidDescriptorSet(
                "invalid interface descriptor",
            ));
        }

        Ok((
            Self {
                interface_number: bytes[2],
                alternate_setting: bytes[3],
                interface_class: bytes[5],
                interface_sub_class: bytes[6],
                interface_protocol: bytes[7],
                interface_index: bytes[8],
            },
            bytes[4],
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbEndpointDescriptor {
    pub address: u8,
    pub attributes: u8,
    pub max_packet_size: u16,
    pub interval: u8,
}

impl UsbEndpointDescriptor {
    const LEN: usize = 7;

    fn to_usb_bytes(&self) -> [u8; Self::LEN] {
        let mut out = [0u8; Self::LEN];

        out[0] = Self::LEN as u8;
        out[1] = USB_DESC_ENDPOINT;
        out[2] = self.address;
        out[3] = self.attributes;
        out[4..6].copy_from_slice(&self.max_packet_size.to_le_bytes());
        out[6] = self.interval;
        out
    }

    fn from_usb_bytes(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() != Self::LEN || bytes[0] != Self::LEN as u8 || bytes[1] != USB_DESC_ENDPOINT
        {
            return Err(ProtocolError::InvalidDescriptorSet(
                "invalid endpoint descriptor",
            ));
        }

        Ok(Self {
            address: bytes[2],
            attributes: bytes[3],
            max_packet_size: read_u16_le(bytes, 4),
            interval: bytes[6],
        })
    }
}

struct OpenInterface {
    descriptor: UsbInterfaceDescriptor,
    expected_endpoints: u8,
    extra_descriptors: Vec<Vec<u8>>,
    endpoints: Vec<UsbEndpointDescriptor>,
}

impl OpenInterface {
    fn finish(self) -> Result<UsbInterface, ProtocolError> {
        if self.endpoints.len() != self.expected_endpoints as usize {
            return Err(ProtocolError::InvalidDescriptorSet(
                "endpoint count mismatch",
            ));
        }

        Ok(UsbInterface {
            descriptor: self.descriptor,
            extra_descriptors: self.extra_descriptors,
            report_descriptor: None,
            endpoints: self.endpoints,
        })
    }
}

fn hid_keyboard_report_descriptor() -> Vec<u8> {
    vec![
        0x05, 0x01, 0x09, 0x06, 0xa1, 0x01, 0x05, 0x07, 0x19, 0xe0, 0x29, 0xe7, 0x15, 0x00, 0x25,
        0x01, 0x75, 0x01, 0x95, 0x08, 0x81, 0x02, 0x95, 0x01, 0x75, 0x08, 0x81, 0x01, 0x95, 0x05,
        0x75, 0x01, 0x05, 0x08, 0x19, 0x01, 0x29, 0x05, 0x91, 0x02, 0x95, 0x01, 0x75, 0x03, 0x91,
        0x01, 0x95, 0x06, 0x75, 0x08, 0x15, 0x00, 0x25, 0x65, 0x05, 0x07, 0x19, 0x00, 0x29, 0x65,
        0x81, 0x00, 0xc0,
    ]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub request_id: u64,
    pub frame_type: FrameType,
    pub direction: Direction,
    pub transfer_type: TransferType,
    pub endpoint: u8,
    pub status: i32,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn new(
        request_id: u64,
        frame_type: FrameType,
        direction: Direction,
        transfer_type: TransferType,
        endpoint: u8,
        status: i32,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            request_id,
            frame_type,
            direction,
            transfer_type,
            endpoint,
            status,
            payload,
        }
    }

    pub fn mock_request(request_id: u64, payload: impl Into<Vec<u8>>) -> Self {
        Self::new(
            request_id,
            FrameType::Request,
            Direction::HostToDevice,
            TransferType::Bulk,
            1,
            0,
            payload.into(),
        )
    }

    pub fn mock_bulk_request(request_id: u64, payload_len: usize) -> Self {
        Self::mock_request(
            request_id,
            MockBulkPayload::synthetic(request_id, payload_len).encode(),
        )
    }

    pub fn mock_response(&self, payload: impl Into<Vec<u8>>) -> Self {
        Self::new(
            self.request_id,
            FrameType::Response,
            Direction::DeviceToHost,
            self.transfer_type,
            self.endpoint,
            0,
            payload.into(),
        )
    }

    pub fn transfer_status(&self) -> Result<TransferStatus, ProtocolError> {
        TransferStatus::try_from(self.status)
    }

    pub fn status_response(&self, status: TransferStatus) -> Self {
        Self::new(
            self.request_id,
            FrameType::Response,
            Direction::DeviceToHost,
            self.transfer_type,
            self.endpoint,
            status as i32,
            Vec::new(),
        )
    }

    pub fn control_request(request_id: u64, setup: UsbControlSetup) -> Self {
        Self::new(
            request_id,
            FrameType::Request,
            setup.transfer_direction(),
            TransferType::Control,
            0,
            0,
            UsbControlTransfer::setup_only(setup).encode(),
        )
    }

    pub fn control_response(&self, status: i32, data: Vec<u8>) -> Self {
        let payload = match UsbControlTransfer::decode(&self.payload) {
            Ok(control) => UsbControlTransfer::new(control.setup, data).encode(),
            Err(_) => data,
        };

        Self::new(
            self.request_id,
            FrameType::Response,
            self.direction,
            TransferType::Control,
            0,
            status,
            payload,
        )
    }

    pub fn hid_keyboard_input_event(request_id: u64, report: HidKeyboardInputReport) -> Self {
        Self::new(
            request_id,
            FrameType::Event,
            Direction::DeviceToHost,
            TransferType::Interrupt,
            0x81,
            0,
            report.encode().to_vec(),
        )
    }

    pub fn attach_request(request_id: u64) -> Self {
        Self::new(
            request_id,
            FrameType::AttachRequest,
            Direction::HostToDevice,
            TransferType::Control,
            0,
            0,
            Vec::new(),
        )
    }

    pub fn attach_response(&self, status: i32) -> Self {
        Self::new(
            self.request_id,
            FrameType::AttachResponse,
            Direction::DeviceToHost,
            TransferType::Control,
            0,
            status,
            Vec::new(),
        )
    }

    pub fn attach_response_with_descriptors(
        &self,
        status: i32,
        descriptors: &UsbDescriptorSet,
    ) -> Result<Self, ProtocolError> {
        Ok(Self::new(
            self.request_id,
            FrameType::AttachResponse,
            Direction::DeviceToHost,
            TransferType::Control,
            0,
            status,
            descriptors.encode()?,
        ))
    }

    pub fn detach_request(request_id: u64) -> Self {
        Self::new(
            request_id,
            FrameType::DetachRequest,
            Direction::HostToDevice,
            TransferType::Control,
            0,
            0,
            Vec::new(),
        )
    }

    pub fn detach_response(&self, status: i32) -> Self {
        Self::new(
            self.request_id,
            FrameType::DetachResponse,
            Direction::DeviceToHost,
            TransferType::Control,
            0,
            status,
            Vec::new(),
        )
    }

    pub fn cancel_request(request_id: u64, transfer_type: TransferType, endpoint: u8) -> Self {
        Self::new(
            request_id,
            FrameType::CancelRequest,
            Direction::HostToDevice,
            transfer_type,
            endpoint,
            0,
            Vec::new(),
        )
    }

    pub fn cancel_response(&self, status: TransferStatus) -> Self {
        Self::new(
            self.request_id,
            FrameType::CancelResponse,
            Direction::DeviceToHost,
            self.transfer_type,
            self.endpoint,
            status as i32,
            Vec::new(),
        )
    }

    pub fn reset_request(request_id: u64, transfer_type: TransferType, endpoint: u8) -> Self {
        Self::new(
            request_id,
            FrameType::ResetRequest,
            Direction::HostToDevice,
            transfer_type,
            endpoint,
            0,
            Vec::new(),
        )
    }

    pub fn reset_response(&self, status: TransferStatus) -> Self {
        Self::new(
            self.request_id,
            FrameType::ResetResponse,
            Direction::DeviceToHost,
            self.transfer_type,
            self.endpoint,
            status as i32,
            Vec::new(),
        )
    }

    pub fn is_lifecycle_frame(&self) -> bool {
        matches!(
            self.frame_type,
            FrameType::AttachRequest
                | FrameType::AttachResponse
                | FrameType::DetachRequest
                | FrameType::DetachResponse
        )
    }

    pub fn is_transfer_control_frame(&self) -> bool {
        matches!(
            self.frame_type,
            FrameType::CancelRequest
                | FrameType::CancelResponse
                | FrameType::ResetRequest
                | FrameType::ResetResponse
        )
    }

    pub fn encode(&self) -> Vec<u8> {
        let payload_len = u32::try_from(self.payload.len()).expect("payload too large");
        let mut out = Vec::with_capacity(HEADER_LEN + self.payload.len());

        out.extend_from_slice(MAGIC);
        out.push(VERSION);
        out.push(self.frame_type as u8);
        out.push(self.direction as u8);
        out.push(self.transfer_type as u8);
        out.extend_from_slice(&self.request_id.to_be_bytes());
        out.push(self.endpoint);
        out.push(0);
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&self.status.to_be_bytes());
        out.extend_from_slice(&payload_len.to_be_bytes());
        out.extend_from_slice(&self.payload);

        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() < HEADER_LEN {
            return Err(ProtocolError::TruncatedHeader {
                actual: bytes.len(),
                expected: HEADER_LEN,
            });
        }

        if &bytes[0..4] != MAGIC {
            return Err(ProtocolError::InvalidMagic);
        }

        if bytes[4] != VERSION {
            return Err(ProtocolError::UnsupportedVersion(bytes[4]));
        }

        let frame_type = FrameType::try_from(bytes[5])?;
        let direction = Direction::try_from(bytes[6])?;
        let transfer_type = TransferType::try_from(bytes[7])?;

        let request_id = u64::from_be_bytes(bytes[8..16].try_into().unwrap());
        let endpoint = bytes[16];
        let status = i32::from_be_bytes(bytes[20..24].try_into().unwrap());
        let payload_len = u32::from_be_bytes(bytes[24..28].try_into().unwrap()) as usize;
        let expected_len = HEADER_LEN + payload_len;

        if bytes.len() != expected_len {
            return Err(ProtocolError::InvalidPayloadLength {
                actual: bytes.len().saturating_sub(HEADER_LEN),
                expected: payload_len,
            });
        }

        Ok(Self {
            request_id,
            frame_type,
            direction,
            transfer_type,
            endpoint,
            status,
            payload: bytes[HEADER_LEN..].to_vec(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    TruncatedHeader { actual: usize, expected: usize },
    InvalidMagic,
    UnsupportedVersion(u8),
    InvalidFrameType(u8),
    InvalidDirection(u8),
    InvalidTransferType(u8),
    InvalidTransferStatus(i32),
    InvalidPayloadLength { actual: usize, expected: usize },
    InvalidDescriptorSet(&'static str),
    InvalidControlTransfer(&'static str),
    InvalidBulkPayload(&'static str),
    InvalidUsbIpPacket(&'static str),
    InvalidHidReport(&'static str),
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TruncatedHeader { actual, expected } => {
                write!(
                    f,
                    "truncated header: got {actual} bytes, expected {expected}"
                )
            }
            Self::InvalidMagic => write!(f, "invalid magic"),
            Self::UnsupportedVersion(version) => write!(f, "unsupported version {version}"),
            Self::InvalidFrameType(value) => write!(f, "invalid frame type {value}"),
            Self::InvalidDirection(value) => write!(f, "invalid direction {value}"),
            Self::InvalidTransferType(value) => write!(f, "invalid transfer type {value}"),
            Self::InvalidTransferStatus(value) => write!(f, "invalid transfer status {value}"),
            Self::InvalidPayloadLength { actual, expected } => {
                write!(
                    f,
                    "invalid payload length: got {actual}, expected {expected}"
                )
            }
            Self::InvalidDescriptorSet(message) => write!(f, "invalid descriptor set: {message}"),
            Self::InvalidControlTransfer(message) => {
                write!(f, "invalid control transfer: {message}")
            }
            Self::InvalidBulkPayload(message) => write!(f, "invalid bulk payload: {message}"),
            Self::InvalidUsbIpPacket(message) => write!(f, "invalid USB/IP packet: {message}"),
            Self::InvalidHidReport(message) => write!(f, "invalid HID report: {message}"),
        }
    }
}

impl std::error::Error for ProtocolError {}

fn take<'a>(bytes: &'a [u8], offset: &mut usize, len: usize) -> Result<&'a [u8], ProtocolError> {
    let end = offset
        .checked_add(len)
        .ok_or(ProtocolError::InvalidDescriptorSet(
            "descriptor offset overflow",
        ))?;
    if end > bytes.len() {
        return Err(ProtocolError::InvalidDescriptorSet(
            "descriptor payload truncated",
        ));
    }

    let slice = &bytes[*offset..end];
    *offset = end;
    Ok(slice)
}

fn descriptor_header(bytes: &[u8], offset: usize) -> Result<(usize, u8), ProtocolError> {
    if offset + 2 > bytes.len() {
        return Err(ProtocolError::InvalidDescriptorSet(
            "descriptor header truncated",
        ));
    }

    let descriptor_len = bytes[offset] as usize;
    if descriptor_len < 2 || offset + descriptor_len > bytes.len() {
        return Err(ProtocolError::InvalidDescriptorSet(
            "invalid descriptor length",
        ));
    }

    Ok((descriptor_len, bytes[offset + 1]))
}

fn read_u16_le(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32_be(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0u32, |acc, byte| {
        acc.rotate_left(5).wrapping_add(u32::from(*byte))
    })
}

fn validate_extra_descriptor(bytes: &[u8]) -> Result<(), ProtocolError> {
    if bytes.len() < 2 || bytes[0] as usize != bytes.len() {
        return Err(ProtocolError::InvalidDescriptorSet(
            "invalid extra descriptor",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_frame() {
        let frame = Frame::mock_request(42, b"mock urb".to_vec());

        let decoded = Frame::decode(&frame.encode()).unwrap();

        assert_eq!(decoded, frame);
    }

    #[test]
    fn round_trips_lifecycle_frames() {
        let frames = [
            Frame::attach_request(1),
            Frame::attach_request(1).attach_response(0),
            Frame::detach_request(2),
            Frame::detach_request(2).detach_response(0),
        ];

        for frame in frames {
            let decoded = Frame::decode(&frame.encode()).unwrap();

            assert_eq!(decoded, frame);
            assert!(decoded.is_lifecycle_frame());
            assert_eq!(decoded.transfer_type, TransferType::Control);
            assert_eq!(decoded.endpoint, 0);
            assert!(decoded.payload.is_empty());
        }
    }

    #[test]
    fn round_trips_transfer_control_frames() {
        let frames = [
            Frame::cancel_request(7, TransferType::Bulk, 1),
            Frame::cancel_request(7, TransferType::Bulk, 1)
                .cancel_response(TransferStatus::Cancelled),
            Frame::reset_request(8, TransferType::Interrupt, 0x81),
            Frame::reset_request(8, TransferType::Interrupt, 0x81)
                .reset_response(TransferStatus::Ok),
        ];

        for frame in frames {
            let decoded = Frame::decode(&frame.encode()).unwrap();

            assert_eq!(decoded, frame);
            assert!(decoded.is_transfer_control_frame());
            assert!(decoded.payload.is_empty());
        }
    }

    #[test]
    fn round_trips_transfer_status_responses() {
        let request = Frame::mock_request(42, b"mock urb".to_vec());
        let statuses = [
            TransferStatus::Ok,
            TransferStatus::Failed,
            TransferStatus::Cancelled,
            TransferStatus::Timeout,
            TransferStatus::Stall,
            TransferStatus::Reset,
        ];

        for status in statuses {
            let decoded = Frame::decode(&request.status_response(status).encode()).unwrap();

            assert_eq!(decoded.frame_type, FrameType::Response);
            assert_eq!(decoded.transfer_status().unwrap(), status);
        }
    }

    #[test]
    fn round_trips_usbip_out_submit() {
        let mut submit = UsbIpCmdSubmit::new(
            7,
            0x0001_0002,
            UsbIpDirection::Out,
            1,
            b"bulk out".to_vec(),
            [0; 8],
        );
        submit.transfer_flags = 0x0000_0200;
        submit.interval = 4;

        let encoded = submit.encode().unwrap();
        let decoded = UsbIpCmdSubmit::decode(&encoded).unwrap();

        assert_eq!(encoded.len(), USBIP_HEADER_LEN + 8);
        assert_eq!(decoded, submit);
        assert_eq!(read_u32_be(&encoded, 0), UsbIpCommand::CmdSubmit as u32);
        assert_eq!(read_u32_be(&encoded, 4), 7);
        assert_eq!(read_u32_be(&encoded, 8), 0x0001_0002);
        assert_eq!(read_u32_be(&encoded, 12), UsbIpDirection::Out as u32);
        assert_eq!(read_u32_be(&encoded, 16), 1);
        assert_eq!(read_u32_be(&encoded, 32), USBIP_NO_ISO_PACKETS);
    }

    #[test]
    fn round_trips_usbip_in_submit_without_payload() {
        let submit = UsbIpCmdSubmit {
            header: UsbIpHeaderBasic {
                command: UsbIpCommand::CmdSubmit,
                seqnum: 8,
                devid: 0x0001_0002,
                direction: UsbIpDirection::In,
                endpoint: 0x81,
            },
            transfer_flags: 0,
            transfer_buffer_length: 8,
            start_frame: 0,
            number_of_packets: USBIP_NO_ISO_PACKETS,
            interval: 10,
            setup: [0; 8],
            transfer_buffer: Vec::new(),
        };

        let encoded = submit.encode().unwrap();
        let decoded = UsbIpCmdSubmit::decode(&encoded).unwrap();

        assert_eq!(encoded.len(), USBIP_HEADER_LEN);
        assert_eq!(decoded, submit);
        assert_eq!(decoded.transfer_buffer_length, 8);
        assert!(decoded.transfer_buffer.is_empty());
    }

    #[test]
    fn round_trips_usbip_ret_submit_with_payload() {
        let submit = UsbIpCmdSubmit {
            header: UsbIpHeaderBasic {
                command: UsbIpCommand::CmdSubmit,
                seqnum: 8,
                devid: 0x0001_0002,
                direction: UsbIpDirection::In,
                endpoint: 0x81,
            },
            transfer_flags: 0,
            transfer_buffer_length: 8,
            start_frame: 0,
            number_of_packets: USBIP_NO_ISO_PACKETS,
            interval: 10,
            setup: [0; 8],
            transfer_buffer: Vec::new(),
        };
        let response = UsbIpRetSubmit::ok_for(&submit, vec![1, 2, 3, 4]);

        let encoded = response.encode().unwrap();
        let decoded = UsbIpRetSubmit::decode(&encoded).unwrap();

        assert_eq!(encoded.len(), USBIP_HEADER_LEN + 4);
        assert_eq!(decoded, response);
        assert_eq!(read_u32_be(&encoded, 0), UsbIpCommand::RetSubmit as u32);
        assert_eq!(read_u32_be(&encoded, 4), submit.header.seqnum);
        assert_eq!(read_u32_be(&encoded, 24), 4);
    }

    #[test]
    fn round_trips_usbip_unlink() {
        let request = UsbIpCmdUnlink::new(11, 0x0001_0002, 7);
        let response = UsbIpRetUnlink::new(request.header.seqnum, 0);

        let decoded_request = UsbIpCmdUnlink::decode(&request.encode()).unwrap();
        let decoded_response = UsbIpRetUnlink::decode(&response.encode()).unwrap();

        assert_eq!(decoded_request, request);
        assert_eq!(decoded_request.unlink_seqnum, 7);
        assert_eq!(decoded_response, response);
        assert_eq!(decoded_response.status, 0);
    }

    #[test]
    fn round_trips_usb_descriptor_set() {
        let descriptors = UsbDescriptorSet::mock_hid_keyboard(0x1209, 0x0001);
        let encoded = descriptors.encode().unwrap();
        let decoded = UsbDescriptorSet::decode(&encoded).unwrap();

        assert_eq!(decoded, descriptors);
        assert_eq!(decoded.device.vendor_id, 0x1209);
        assert_eq!(decoded.device.product_id, 0x0001);
        assert_eq!(decoded.configurations.len(), 1);
        assert_eq!(decoded.configurations[0].interfaces.len(), 1);
        assert_eq!(decoded.configurations[0].interfaces[0].endpoints.len(), 1);
        assert_eq!(
            decoded
                .report_descriptor_bytes(0, 0, 0)
                .expect("report descriptor"),
            descriptors
                .report_descriptor_bytes(0, 0, 0)
                .expect("report descriptor")
        );
        assert_eq!(
            decoded.configurations[0].interfaces[0].extra_descriptors[0][1],
            0x21
        );
    }

    #[test]
    fn attach_response_can_carry_usb_descriptors() {
        let descriptors = UsbDescriptorSet::mock_hid_keyboard(0x1234, 0x5678);
        let response = Frame::attach_request(10)
            .attach_response_with_descriptors(0, &descriptors)
            .unwrap();
        let decoded_frame = Frame::decode(&response.encode()).unwrap();
        let decoded_descriptors = UsbDescriptorSet::decode(&decoded_frame.payload).unwrap();

        assert_eq!(decoded_frame.frame_type, FrameType::AttachResponse);
        assert_eq!(decoded_frame.status, 0);
        assert_eq!(decoded_descriptors, descriptors);
    }

    #[test]
    fn round_trips_control_setup_packet() {
        let setup = UsbControlSetup::get_descriptor(UsbDescriptorType::Device, 0, 18);
        let decoded = UsbControlSetup::decode(&setup.encode()).unwrap();

        assert_eq!(decoded, setup);
        assert_eq!(
            decoded.standard_request().unwrap(),
            UsbStandardRequest::GetDescriptor
        );
        assert_eq!(
            decoded.descriptor_type().unwrap(),
            UsbDescriptorType::Device
        );
        assert_eq!(decoded.descriptor_index(), 0);
        assert_eq!(decoded.transfer_direction(), Direction::DeviceToHost);
    }

    #[test]
    fn frame_can_carry_control_transfer() {
        let frame = Frame::control_request(
            5,
            UsbControlSetup::get_descriptor(UsbDescriptorType::Configuration, 0, 64),
        );
        let decoded = Frame::decode(&frame.encode()).unwrap();
        let control = UsbControlTransfer::decode(&decoded.payload).unwrap();

        assert_eq!(decoded.frame_type, FrameType::Request);
        assert_eq!(decoded.transfer_type, TransferType::Control);
        assert_eq!(decoded.endpoint, 0);
        assert_eq!(
            control.setup.descriptor_type().unwrap(),
            UsbDescriptorType::Configuration
        );
    }

    #[test]
    fn frame_can_carry_mock_bulk_payload() {
        let frame = Frame::mock_bulk_request(9, 4096);
        let decoded = Frame::decode(&frame.encode()).unwrap();
        let payload = MockBulkPayload::decode(&decoded.payload).unwrap();

        assert_eq!(decoded.frame_type, FrameType::Request);
        assert_eq!(decoded.transfer_type, TransferType::Bulk);
        assert_eq!(decoded.endpoint, 1);
        assert_eq!(payload.data.len(), 4096);
        assert_eq!(payload, MockBulkPayload::synthetic(9, 4096));
    }

    #[test]
    fn rejects_mock_bulk_checksum_mismatch() {
        let mut encoded = MockBulkPayload::synthetic(1, 16).encode();
        let last = encoded.last_mut().unwrap();
        *last = last.wrapping_add(1);

        assert_eq!(
            MockBulkPayload::decode(&encoded).unwrap_err(),
            ProtocolError::InvalidBulkPayload("checksum mismatch")
        );
    }

    #[test]
    fn frame_can_request_hid_report_descriptor() {
        let frame = Frame::control_request(
            6,
            UsbControlSetup::get_interface_descriptor(UsbDescriptorType::Report, 0, 0, 63),
        );
        let decoded = Frame::decode(&frame.encode()).unwrap();
        let control = UsbControlTransfer::decode(&decoded.payload).unwrap();

        assert_eq!(decoded.direction, Direction::DeviceToHost);
        assert_eq!(control.setup.request_type, 0x81);
        assert_eq!(control.setup.index, 0);
        assert_eq!(
            control.setup.descriptor_type().unwrap(),
            UsbDescriptorType::Report
        );
    }

    #[test]
    fn frame_can_carry_hid_keyboard_input_event() {
        let report = HidKeyboardInputReport::key_press(HidKeyboardInputReport::KEY_A);
        let frame = Frame::hid_keyboard_input_event(0, report);
        let decoded = Frame::decode(&frame.encode()).unwrap();
        let decoded_report = HidKeyboardInputReport::decode(&decoded.payload).unwrap();

        assert_eq!(decoded.frame_type, FrameType::Event);
        assert_eq!(decoded.direction, Direction::DeviceToHost);
        assert_eq!(decoded.transfer_type, TransferType::Interrupt);
        assert_eq!(decoded.endpoint, 0x81);
        assert_eq!(decoded_report, report);
        assert_eq!(decoded_report.keycodes[0], HidKeyboardInputReport::KEY_A);
    }

    #[test]
    fn exposes_raw_descriptor_bytes_for_get_descriptor() {
        let descriptors = UsbDescriptorSet::mock_hid_keyboard(0x1234, 0x5678);
        let device = descriptors.device_descriptor_bytes().unwrap();
        let configuration = descriptors.configuration_descriptor_bytes(0).unwrap();
        let report = descriptors.report_descriptor_bytes(0, 0, 0).unwrap();

        assert_eq!(device[0], 18);
        assert_eq!(device[1], USB_DESC_DEVICE);
        assert_eq!(read_u16_le(&device, 8), 0x1234);
        assert_eq!(read_u16_le(&device, 10), 0x5678);
        assert_eq!(configuration[0], 9);
        assert_eq!(configuration[1], USB_DESC_CONFIGURATION);
        assert_eq!(
            usize::from(read_u16_le(&configuration, 2)),
            configuration.len()
        );
        assert_eq!(report.len(), 63);
        assert_eq!(report[0..4], [0x05, 0x01, 0x09, 0x06]);
    }

    #[test]
    fn rejects_descriptor_endpoint_count_mismatch() {
        let descriptors = UsbDescriptorSet::mock_hid_keyboard(0x1209, 0x0001);
        let mut encoded = descriptors.encode().unwrap();
        let endpoint_count_offset =
            8 + UsbDeviceDescriptor::LEN + UsbConfigurationDescriptor::LEN + 4;
        encoded[endpoint_count_offset] = 2;

        let err = UsbDescriptorSet::decode(&encoded).unwrap_err();

        assert_eq!(
            err,
            ProtocolError::InvalidDescriptorSet("endpoint count mismatch")
        );
    }

    #[test]
    fn rejects_truncated_header() {
        let err = Frame::decode(b"WHY1").unwrap_err();

        assert_eq!(
            err,
            ProtocolError::TruncatedHeader {
                actual: 4,
                expected: HEADER_LEN
            }
        );
    }

    #[test]
    fn rejects_payload_length_mismatch() {
        let mut encoded = Frame::mock_request(7, b"abc".to_vec()).encode();
        encoded.pop();

        let err = Frame::decode(&encoded).unwrap_err();

        assert_eq!(
            err,
            ProtocolError::InvalidPayloadLength {
                actual: 2,
                expected: 3
            }
        );
    }
}
