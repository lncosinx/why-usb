mod config;
mod driver_backend;
mod endpoint_queue;
mod ioctl;
mod logging;
mod mapped_ring;
mod session;
mod usbip_session;
#[cfg(windows)]
mod windows_driver_backend;

use config::{DaemonConfig, DaemonProtocolMode, DriverBackendKind};
use driver_backend::{DriverBackend, MockDriverBackend};
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    logging::init();
    info!("starting why_usb user-mode daemon");

    let config = DaemonConfig::from_env()?;
    let driver: Arc<dyn DriverBackend> = match config.driver_backend {
        DriverBackendKind::Mock => Arc::new(MockDriverBackend::init()?),
        DriverBackendKind::Windows => {
            build_windows_backend(&config.driver_device_path, config.map_shared_memory)?
        }
    };
    info!("driver backend initialized successfully");

    let listener = TcpListener::bind(config.bind_addr).await?;
    info!(bind_addr = %config.bind_addr, "listening for client connections");

    let (stream, addr) = listener.accept().await?;
    info!(peer_addr = %addr, "accepted client connection");

    match config.protocol_mode {
        DaemonProtocolMode::Why => {
            session::run_single_client(
                stream,
                driver,
                config.attach_device,
                config.mock_hid_keycodes,
                config.mock_transfer_outcomes,
            )
            .await?;
        }
        DaemonProtocolMode::UsbIp => {
            usbip_session::run_single_usbip_client(
                stream,
                driver,
                config.attach_device,
                config.mock_hid_keycodes,
            )
            .await?;
        }
    }

    info!("shutdown complete");
    Ok(())
}

#[cfg(windows)]
fn build_windows_backend(
    device_path: &str,
    map_shared_memory: bool,
) -> Result<Arc<dyn DriverBackend>, Box<dyn std::error::Error>> {
    let mut backend = windows_driver_backend::WindowsDriverBackend::open(device_path)?;
    if map_shared_memory {
        backend.map_shared_memory()?;
    }
    Ok(Arc::new(backend))
}

#[cfg(not(windows))]
fn build_windows_backend(
    _device_path: &str,
    _map_shared_memory: bool,
) -> Result<Arc<dyn DriverBackend>, Box<dyn std::error::Error>> {
    Err("Windows driver backend is only available on Windows".into())
}
