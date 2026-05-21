use std::env;
use std::net::SocketAddr;
use std::time::Duration;

pub struct ClientConfig {
    pub server_addr: SocketAddr,
    pub vhci_backend: VhciBackendKind,
    pub vhci_probe_only: bool,
    pub vhci_device_id: u32,
    pub mock_frame_limit: Option<u64>,
    pub mock_frame_interval: Duration,
    pub mock_bulk_payload_len: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VhciBackendKind {
    Mock,
    Linux,
}

impl VhciBackendKind {
    fn from_env_value(value: &str) -> Result<Self, Box<dyn std::error::Error>> {
        match value.to_ascii_lowercase().as_str() {
            "mock" => Ok(Self::Mock),
            "linux" | "vhci" | "vhci-hcd" => Ok(Self::Linux),
            other => Err(format!("unsupported WHY_USB_VHCI_BACKEND value: {other}").into()),
        }
    }
}

impl ClientConfig {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let server_addr = env::args()
            .nth(1)
            .or_else(|| env::var("WHY_USB_SERVER_ADDR").ok())
            .unwrap_or_else(|| "127.0.0.1:3000".to_string())
            .parse()?;
        let vhci_backend = env::var("WHY_USB_VHCI_BACKEND")
            .ok()
            .map(|value| VhciBackendKind::from_env_value(&value))
            .transpose()?
            .unwrap_or(VhciBackendKind::Mock);
        let vhci_probe_only = env_flag("WHY_USB_VHCI_PROBE_ONLY");
        let vhci_device_id = env::var("WHY_USB_VHCI_DEVID")
            .ok()
            .map(|value| parse_u32(&value))
            .transpose()?
            .unwrap_or(0x0001_0001);
        let mock_frame_limit = env::var("WHY_USB_MOCK_FRAME_LIMIT")
            .ok()
            .map(|value| value.parse())
            .transpose()?;
        let mock_frame_interval_ms = env::var("WHY_USB_MOCK_FRAME_INTERVAL_MS")
            .ok()
            .map(|value| value.parse())
            .transpose()?
            .unwrap_or(5_000);
        let mock_bulk_payload_len = env::var("WHY_USB_MOCK_BULK_BYTES")
            .ok()
            .map(|value| value.parse())
            .transpose()?;

        Ok(Self {
            server_addr,
            vhci_backend,
            vhci_probe_only,
            vhci_device_id,
            mock_frame_limit,
            mock_frame_interval: Duration::from_millis(mock_frame_interval_ms),
            mock_bulk_payload_len,
        })
    }
}

fn parse_u32(value: &str) -> Result<u32, Box<dyn std::error::Error>> {
    if let Some(hex) = value.strip_prefix("0x") {
        return Ok(u32::from_str_radix(hex, 16)?);
    }

    Ok(value.parse()?)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vhci_backend_kind() {
        assert_eq!(
            VhciBackendKind::from_env_value("mock").unwrap(),
            VhciBackendKind::Mock
        );
        assert_eq!(
            VhciBackendKind::from_env_value("vhci-hcd").unwrap(),
            VhciBackendKind::Linux
        );
        assert!(VhciBackendKind::from_env_value("other").is_err());
    }

    #[test]
    fn parses_vhci_device_id() {
        assert_eq!(parse_u32("0x00010002").unwrap(), 0x0001_0002);
        assert_eq!(parse_u32("65538").unwrap(), 65_538);
    }
}
