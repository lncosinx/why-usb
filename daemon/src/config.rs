use crate::driver_backend::DeviceAttachRequest;
use protocol::{HidKeyboardInputReport, TransferStatus};
use std::env;
use std::net::SocketAddr;

pub struct DaemonConfig {
    pub bind_addr: SocketAddr,
    pub protocol_mode: DaemonProtocolMode,
    pub driver_backend: DriverBackendKind,
    pub driver_device_path: String,
    pub map_shared_memory: bool,
    pub attach_device: Option<DeviceAttachRequest>,
    pub mock_hid_keycodes: Vec<u8>,
    pub mock_transfer_outcomes: Vec<MockTransferOutcomeRule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonProtocolMode {
    Why,
    UsbIp,
}

impl DaemonProtocolMode {
    fn from_env_value(value: &str) -> Result<Self, Box<dyn std::error::Error>> {
        match value.to_ascii_lowercase().as_str() {
            "why" | "why-usb" | "mock" => Ok(Self::Why),
            "usbip" | "usb/ip" | "vhci" | "vhci-hcd" => Ok(Self::UsbIp),
            other => Err(format!("unsupported WHY_USB_DAEMON_PROTOCOL value: {other}").into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockTransferOutcomeRule {
    pub request_id: u64,
    pub outcome: MockTransferOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockTransferOutcome {
    Status(TransferStatus),
    ShortPacket(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverBackendKind {
    Mock,
    Windows,
}

impl DriverBackendKind {
    fn from_env_value(value: &str) -> Result<Self, Box<dyn std::error::Error>> {
        match value.to_ascii_lowercase().as_str() {
            "mock" => Ok(Self::Mock),
            "windows" | "win" | "ioctl" => Ok(Self::Windows),
            other => Err(format!("unsupported WHY_USB_DRIVER_BACKEND value: {other}").into()),
        }
    }
}

impl DaemonConfig {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let bind_addr = env::args()
            .nth(1)
            .or_else(|| env::var("WHY_USB_BIND_ADDR").ok())
            .unwrap_or_else(|| "0.0.0.0:3000".to_string())
            .parse()?;
        let protocol_mode = env::var("WHY_USB_DAEMON_PROTOCOL")
            .ok()
            .map(|value| DaemonProtocolMode::from_env_value(&value))
            .transpose()?
            .unwrap_or(DaemonProtocolMode::Why);
        let driver_backend = env::var("WHY_USB_DRIVER_BACKEND")
            .ok()
            .map(|value| DriverBackendKind::from_env_value(&value))
            .transpose()?
            .unwrap_or(DriverBackendKind::Mock);
        let driver_device_path =
            env::var("WHY_USB_DRIVER_DEVICE").unwrap_or_else(|_| r"\\.\why_usb_vhci".to_string());
        let map_shared_memory = env_flag("WHY_USB_MAP_SHARED_MEMORY");
        let attach_device = env::var("WHY_USB_ATTACH_DEVICE")
            .ok()
            .map(|value| parse_attach_device(&value))
            .transpose()?;
        let mock_hid_keycodes = env::var("WHY_USB_MOCK_HID_KEYS")
            .ok()
            .map(|value| parse_hid_keycodes(&value))
            .transpose()?
            .unwrap_or_else(|| vec![HidKeyboardInputReport::KEY_A]);
        let mock_transfer_outcomes = env::var("WHY_USB_MOCK_TRANSFER_OUTCOMES")
            .ok()
            .map(|value| parse_mock_transfer_outcomes(&value))
            .transpose()?
            .unwrap_or_default();

        Ok(Self {
            bind_addr,
            protocol_mode,
            driver_backend,
            driver_device_path,
            map_shared_memory,
            attach_device,
            mock_hid_keycodes,
            mock_transfer_outcomes,
        })
    }
}

fn env_flag(name: &str) -> bool {
    env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn parse_attach_device(value: &str) -> Result<DeviceAttachRequest, Box<dyn std::error::Error>> {
    let mut parts = value.split(':');
    let vendor_id = parse_u16(parts.next(), "vendor id")?;
    let product_id = parse_u16(parts.next(), "product id")?;
    let bus_id = parse_u8(parts.next().unwrap_or("0"), "bus id")?;
    let port_id = parse_u8(parts.next().unwrap_or("0"), "port id")?;

    if parts.next().is_some() {
        return Err("WHY_USB_ATTACH_DEVICE must be vid:pid[:bus[:port]]".into());
    }

    let flags = env::var("WHY_USB_ATTACH_FLAGS")
        .ok()
        .map(|value| parse_u16(Some(value.as_str()), "attach flags"))
        .transpose()?
        .unwrap_or(0);

    Ok(DeviceAttachRequest {
        vendor_id,
        product_id,
        bus_id,
        port_id,
        flags,
    })
}

fn parse_u16(value: Option<&str>, field: &'static str) -> Result<u16, Box<dyn std::error::Error>> {
    let value = value.ok_or_else(|| format!("missing {field}"))?;
    let trimmed = value.trim_start_matches("0x");

    Ok(u16::from_str_radix(trimmed, 16).or_else(|_| value.parse())?)
}

fn parse_u8(value: &str, field: &'static str) -> Result<u8, Box<dyn std::error::Error>> {
    let trimmed = value.trim_start_matches("0x");

    u8::from_str_radix(trimmed, 16)
        .or_else(|_| value.parse())
        .map_err(|e| format!("invalid {field}: {e}").into())
}

fn parse_hid_keycodes(value: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let keycodes = value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(parse_hid_keycode)
        .collect::<Result<Vec<_>, _>>()?;

    if keycodes.is_empty() {
        return Err("WHY_USB_MOCK_HID_KEYS must include at least one key".into());
    }

    Ok(keycodes)
}

fn parse_hid_keycode(value: &str) -> Result<u8, Box<dyn std::error::Error>> {
    let lower = value.to_ascii_lowercase();
    let keycode = match lower.as_str() {
        "enter" | "return" => 0x28,
        "escape" | "esc" => 0x29,
        "backspace" => 0x2a,
        "tab" => 0x2b,
        "space" => 0x2c,
        letter if letter.len() == 1 => {
            let byte = letter.as_bytes()[0];
            if byte.is_ascii_lowercase() {
                HidKeyboardInputReport::KEY_A + (byte - b'a')
            } else {
                parse_u8(value, "HID keycode")?
            }
        }
        _ => parse_u8(value, "HID keycode")?,
    };

    Ok(keycode)
}

fn parse_mock_transfer_outcomes(
    value: &str,
) -> Result<Vec<MockTransferOutcomeRule>, Box<dyn std::error::Error>> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(parse_mock_transfer_outcome)
        .collect()
}

fn parse_mock_transfer_outcome(
    value: &str,
) -> Result<MockTransferOutcomeRule, Box<dyn std::error::Error>> {
    let (request_id, outcome) = value
        .split_once('=')
        .ok_or("mock transfer outcome must be request_id=outcome")?;
    let request_id = request_id.trim().parse()?;
    let outcome = parse_mock_transfer_outcome_value(outcome.trim())?;

    Ok(MockTransferOutcomeRule {
        request_id,
        outcome,
    })
}

fn parse_mock_transfer_outcome_value(
    value: &str,
) -> Result<MockTransferOutcome, Box<dyn std::error::Error>> {
    let lower = value.to_ascii_lowercase();

    if let Some(len) = lower.strip_prefix("short:") {
        return Ok(MockTransferOutcome::ShortPacket(len.parse()?));
    }

    let status = match lower.as_str() {
        "ok" => TransferStatus::Ok,
        "failed" | "fail" | "error" => TransferStatus::Failed,
        "cancelled" | "canceled" | "cancel" => TransferStatus::Cancelled,
        "timeout" => TransferStatus::Timeout,
        "stall" | "stalled" => TransferStatus::Stall,
        "reset" => TransferStatus::Reset,
        _ => return Err(format!("unsupported mock transfer outcome: {value}").into()),
    };

    Ok(MockTransferOutcome::Status(status))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_attach_device_selector() {
        let request = parse_attach_device("1234:5678:1:2").unwrap();

        assert_eq!(
            request,
            DeviceAttachRequest {
                vendor_id: 0x1234,
                product_id: 0x5678,
                bus_id: 1,
                port_id: 2,
                flags: 0,
            }
        );
    }

    #[test]
    fn parses_daemon_protocol_mode() {
        assert_eq!(
            DaemonProtocolMode::from_env_value("why-usb").unwrap(),
            DaemonProtocolMode::Why
        );
        assert_eq!(
            DaemonProtocolMode::from_env_value("usbip").unwrap(),
            DaemonProtocolMode::UsbIp
        );
        assert!(DaemonProtocolMode::from_env_value("other").is_err());
    }

    #[test]
    fn rejects_too_many_attach_fields() {
        assert!(parse_attach_device("1234:5678:1:2:3").is_err());
    }

    #[test]
    fn parses_mock_hid_keycodes() {
        let keycodes = parse_hid_keycodes("a, enter, 0x2c").unwrap();

        assert_eq!(keycodes, vec![0x04, 0x28, 0x2c]);
    }

    #[test]
    fn rejects_empty_mock_hid_keycodes() {
        assert!(parse_hid_keycodes(" , ").is_err());
    }

    #[test]
    fn parses_mock_transfer_outcomes() {
        let outcomes = parse_mock_transfer_outcomes("7=timeout, 8=stall, 9=short:4").unwrap();

        assert_eq!(
            outcomes,
            vec![
                MockTransferOutcomeRule {
                    request_id: 7,
                    outcome: MockTransferOutcome::Status(TransferStatus::Timeout),
                },
                MockTransferOutcomeRule {
                    request_id: 8,
                    outcome: MockTransferOutcome::Status(TransferStatus::Stall),
                },
                MockTransferOutcomeRule {
                    request_id: 9,
                    outcome: MockTransferOutcome::ShortPacket(4),
                },
            ]
        );
    }

    #[test]
    fn rejects_unknown_mock_transfer_outcome() {
        assert!(parse_mock_transfer_outcomes("7=surprise").is_err());
    }
}
