use crate::config::VhciBackendKind;
use bytes::Bytes;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use tracing::{info, warn};

pub struct VhciAdapter {
    backend: VhciBackend,
}

enum VhciBackend {
    Mock { sender: mpsc::Sender<Bytes> },
    Linux { status: LinuxVhciStatus },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxVhciStatus {
    pub module_loaded: bool,
    pub status_path: Option<PathBuf>,
    pub attach_path: Option<PathBuf>,
    pub ports: Vec<LinuxVhciPort>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxVhciPort {
    pub hub: String,
    pub port: u16,
    pub status: String,
    pub speed: u32,
    pub device_id: u32,
    pub socket_fd: i64,
    pub local_busid: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinuxVhciAttachRequest {
    pub port: u16,
    pub socket_fd: i32,
    pub device_id: u32,
    pub speed: LinuxUsbSpeed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
#[allow(dead_code)]
pub enum LinuxUsbSpeed {
    Low = 1,
    Full = 2,
    High = 3,
    Wireless = 4,
    Super = 5,
    SuperPlus = 6,
}

impl VhciAdapter {
    pub fn new(
        backend: VhciBackendKind,
    ) -> Result<(Self, mpsc::Receiver<Bytes>), Box<dyn std::error::Error>> {
        let (tx, rx) = mpsc::channel(100);
        let backend = match backend {
            VhciBackendKind::Mock => VhciBackend::Mock { sender: tx },
            VhciBackendKind::Linux => {
                let status = LinuxVhciStatus::probe_default();
                if !status.is_ready() {
                    return Err(linux_vhci_not_ready_message(&status).into());
                }

                info!(
                    status_path = ?status.status_path,
                    attach_path = ?status.attach_path,
                    ports = status.ports.len(),
                    free_ports = status.free_ports().len(),
                    "Linux vhci-hcd backend selected"
                );
                if status.free_ports().is_empty() {
                    warn!("Linux vhci-hcd has no free ports in status output");
                } else if let Some(port) = status.first_free_port() {
                    let attach_preview = LinuxVhciAttachRequest {
                        port: port.port,
                        socket_fd: 0,
                        device_id: 0,
                        speed: port.default_attach_speed(),
                    };
                    info!(
                        port = port.port,
                        hub = %port.hub,
                        attach_format = %attach_preview.to_sysfs_line(),
                        "Linux vhci-hcd free port discovered"
                    );
                }
                VhciBackend::Linux { status }
            }
        };

        Ok((Self { backend }, rx))
    }

    pub async fn inject_urb(&self, data: Bytes) -> Result<(), &'static str> {
        match &self.backend {
            VhciBackend::Mock { sender } => {
                if sender.send(data).await.is_err() {
                    return Err("failed to mock inject URB: channel closed");
                }
                Ok(())
            }
            VhciBackend::Linux { status } => {
                let _ = status;
                Err("Linux vhci-hcd URB injection is not wired yet; usbip/vhci attach plumbing is the next implementation step")
            }
        }
    }

    pub fn prepare_linux_attach_dry_run(
        &self,
        socket_fd: i32,
        device_id: u32,
    ) -> Option<LinuxVhciAttachRequest> {
        let VhciBackend::Linux { status } = &self.backend else {
            return None;
        };
        let Some(port) = status.first_free_port() else {
            warn!("cannot prepare Linux vhci-hcd attach request: no free port");
            return None;
        };
        let request = LinuxVhciAttachRequest {
            port: port.port,
            socket_fd,
            device_id,
            speed: port.default_attach_speed(),
        };

        info!(
            attach_path = ?status.attach_path,
            port = request.port,
            socket_fd = request.socket_fd,
            device_id = request.device_id,
            speed = request.speed as u32,
            sysfs_line = %request.to_sysfs_line(),
            "prepared Linux vhci-hcd attach dry run"
        );
        Some(request)
    }
}

impl LinuxVhciStatus {
    pub fn probe_default() -> Self {
        Self::probe_paths(
            Path::new("/sys/module/vhci_hcd"),
            &[
                Path::new("/sys/devices/platform/vhci_hcd/status"),
                Path::new("/sys/devices/platform/vhci_hcd.0/status"),
            ],
        )
    }

    fn probe_paths(module_path: &Path, status_paths: &[&Path]) -> Self {
        let status_path = status_paths
            .iter()
            .find(|path| path.exists())
            .map(|path| path.to_path_buf());
        let attach_path = status_path
            .as_ref()
            .and_then(|path| path.parent())
            .map(|parent| parent.join("attach"))
            .filter(|path| path.exists());
        let ports = status_path
            .as_ref()
            .and_then(|path| fs::read_to_string(path).ok())
            .map(|status| parse_vhci_status(&status))
            .unwrap_or_default();

        Self {
            module_loaded: module_path.exists(),
            status_path,
            attach_path,
            ports,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.module_loaded && self.status_path.is_some() && self.attach_path.is_some()
    }

    pub fn free_ports(&self) -> Vec<&LinuxVhciPort> {
        self.ports.iter().filter(|port| port.is_free()).collect()
    }

    pub fn first_free_port(&self) -> Option<&LinuxVhciPort> {
        self.ports.iter().find(|port| port.is_free())
    }
}

impl LinuxVhciPort {
    pub fn is_free(&self) -> bool {
        self.device_id == 0 && self.socket_fd == 0 && self.local_busid == "0-0"
    }

    fn default_attach_speed(&self) -> LinuxUsbSpeed {
        if self.hub == "ss" {
            LinuxUsbSpeed::Super
        } else {
            LinuxUsbSpeed::High
        }
    }
}

impl LinuxVhciAttachRequest {
    pub fn to_sysfs_line(&self) -> String {
        format!(
            "{} {} {} {}",
            self.port, self.socket_fd, self.device_id, self.speed as u32
        )
    }
}

fn linux_vhci_not_ready_message(status: &LinuxVhciStatus) -> String {
    let module_hint = if status.module_loaded {
        "vhci_hcd module loaded"
    } else {
        "vhci_hcd module not loaded"
    };
    let status_hint = if status.status_path.is_some() {
        "found vhci status path"
    } else {
        "missing /sys/devices/platform/vhci_hcd*/status"
    };
    let attach_hint = if status.attach_path.is_some() {
        "found vhci attach path"
    } else {
        "missing /sys/devices/platform/vhci_hcd*/attach"
    };

    format!(
        "Linux vhci-hcd backend is not ready: {module_hint}, {status_hint}, {attach_hint}. Try `sudo modprobe vhci_hcd`, then rerun with WHY_USB_VHCI_BACKEND=linux."
    )
}

fn parse_vhci_status(status: &str) -> Vec<LinuxVhciPort> {
    status.lines().filter_map(parse_vhci_status_line).collect()
}

fn parse_vhci_status_line(line: &str) -> Option<LinuxVhciPort> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("hub ") {
        return None;
    }

    let mut parts = trimmed.split_whitespace();
    let hub = parts.next()?.to_string();
    let port = parse_vhci_status_decimal(parts.next()?)? as u16;
    let status = parts.next()?.to_string();
    let speed = parse_vhci_status_decimal(parts.next()?)?;
    let device_id = parse_vhci_status_hex(parts.next()?)?;
    let socket_fd = parse_vhci_status_i64(parts.next()?)?;
    let local_busid = parts.next().unwrap_or("").to_string();

    Some(LinuxVhciPort {
        hub,
        port,
        status,
        speed,
        device_id,
        socket_fd,
        local_busid,
    })
}

fn parse_vhci_status_decimal(value: &str) -> Option<u32> {
    value.parse().ok()
}

fn parse_vhci_status_hex(value: &str) -> Option<u32> {
    u32::from_str_radix(value, 16).ok()
}

fn parse_vhci_status_i64(value: &str) -> Option<i64> {
    value
        .parse()
        .ok()
        .or_else(|| i64::from_str_radix(value, 16).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn linux_probe_detects_ready_status() {
        let temp = temp_dir("ready");
        let module_path = temp.join("module");
        let sysfs_dir = temp.join("vhci_hcd.0");
        let attach_path = sysfs_dir.join("attach");
        let status_path = sysfs_dir.join("status");

        fs::create_dir_all(&module_path).unwrap();
        fs::create_dir_all(&sysfs_dir).unwrap();
        fs::write(&attach_path, b"").unwrap();
        fs::write(
            &status_path,
            b"hub port sta spd dev sockfd local_busid\nhs 0000 000 000 00000000 000000 0-0\n",
        )
        .unwrap();

        let status = LinuxVhciStatus::probe_paths(&module_path, &[status_path.as_path()]);

        assert!(status.module_loaded);
        assert_eq!(status.status_path, Some(status_path));
        assert_eq!(status.attach_path, Some(attach_path));
        assert!(status.is_ready());
        assert_eq!(status.ports.len(), 1);
        assert_eq!(status.first_free_port().unwrap().port, 0);
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn linux_probe_requires_module_status_and_attach_path() {
        let temp = temp_dir("missing");
        let module_path = temp.join("module");
        let status_path = temp.join("status");

        let status = LinuxVhciStatus::probe_paths(&module_path, &[status_path.as_path()]);

        assert!(!status.module_loaded);
        assert_eq!(status.status_path, None);
        assert_eq!(status.attach_path, None);
        assert!(!status.is_ready());
        assert!(linux_vhci_not_ready_message(&status).contains("modprobe vhci_hcd"));
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn parses_vhci_status_ports() {
        let ports = parse_vhci_status(
            "hub port sta spd dev sockfd local_busid\n\
             hs 0000 000 000 00000000 000000 0-0\n\
             hs 0001 004 003 00010002 000042 1-2\n",
        );

        assert_eq!(ports.len(), 2);
        assert!(ports[0].is_free());
        assert_eq!(ports[1].hub, "hs");
        assert_eq!(ports[1].port, 1);
        assert_eq!(ports[1].status, "004");
        assert_eq!(ports[1].speed, 3);
        assert_eq!(ports[1].device_id, 65538);
        assert_eq!(ports[1].socket_fd, 42);
        assert_eq!(ports[1].local_busid, "1-2");
        assert!(!ports[1].is_free());
    }

    #[test]
    fn formats_attach_request_for_sysfs() {
        let request = LinuxVhciAttachRequest {
            port: 2,
            socket_fd: 42,
            device_id: 0x0001_0002,
            speed: LinuxUsbSpeed::High,
        };

        assert_eq!(request.to_sysfs_line(), "2 42 65538 3");
    }

    #[test]
    fn prepares_linux_attach_dry_run_from_first_free_port() {
        let status = LinuxVhciStatus {
            module_loaded: true,
            status_path: Some(PathBuf::from("/sys/devices/platform/vhci_hcd.0/status")),
            attach_path: Some(PathBuf::from("/sys/devices/platform/vhci_hcd.0/attach")),
            ports: parse_vhci_status(
                "hub port sta spd dev sockfd local_busid\n\
                 hs 0000 004 003 00010002 000042 1-2\n\
                 ss 0008 000 000 00000000 000000 0-0\n",
            ),
        };
        let adapter = VhciAdapter {
            backend: VhciBackend::Linux { status },
        };

        let request = adapter
            .prepare_linux_attach_dry_run(55, 0x0002_0003)
            .unwrap();

        assert_eq!(request.port, 8);
        assert_eq!(request.socket_fd, 55);
        assert_eq!(request.device_id, 0x0002_0003);
        assert_eq!(request.speed, LinuxUsbSpeed::Super);
        assert_eq!(request.to_sysfs_line(), "8 55 131075 5");
    }

    #[test]
    fn exposes_linux_usb_speed_values() {
        assert_eq!(LinuxUsbSpeed::Low as u32, 1);
        assert_eq!(LinuxUsbSpeed::Full as u32, 2);
        assert_eq!(LinuxUsbSpeed::High as u32, 3);
        assert_eq!(LinuxUsbSpeed::Wireless as u32, 4);
        assert_eq!(LinuxUsbSpeed::Super as u32, 5);
        assert_eq!(LinuxUsbSpeed::SuperPlus as u32, 6);
    }

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("why-usb-vhci-test-{name}-{unique}"))
    }
}
