mod config;
mod logging;
mod session;
mod vhci;

use config::ClientConfig;
use tracing::info;
use vhci::VhciAdapter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    logging::init();
    info!("starting why_usb Linux client");

    let config = ClientConfig::from_env()?;
    let (vhci_adapter, vhci_receiver) = VhciAdapter::new(config.vhci_backend)?;
    if config.vhci_probe_only {
        info!("VHCI backend probe completed");
        return Ok(());
    }

    session::run_client_session(
        config.server_addr,
        vhci_adapter,
        vhci_receiver,
        config.mock_frame_limit,
        config.mock_frame_interval,
        config.mock_bulk_payload_len,
        config.vhci_device_id,
    )
    .await?;

    info!("shutdown complete");
    Ok(())
}
