use crate::vhci::VhciAdapter;
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use protocol::{
    Frame, FrameType, HidKeyboardInputReport, MockBulkPayload, TransferStatus, TransferType,
    UsbControlSetup, UsbControlTransfer, UsbDescriptorSet, UsbDescriptorType, UsbStandardRequest,
};
use std::collections::HashMap;
use std::net::SocketAddr;
#[cfg(unix)]
use std::os::fd::AsRawFd;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::sleep;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tracing::{error, info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionState {
    Connecting,
    Connected,
    Attached,
    Detaching,
    Closed,
}

pub async fn run_client_session(
    server_addr: SocketAddr,
    vhci_adapter: VhciAdapter,
    mut vhci_receiver: tokio::sync::mpsc::Receiver<Bytes>,
    mock_frame_limit: Option<u64>,
    mock_frame_interval: Duration,
    mock_bulk_payload_len: Option<usize>,
    vhci_device_id: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut state = SessionState::Connecting;
    info!(?state, %server_addr, "attempting to connect to server");

    let stream = connect_with_retry(server_addr).await;
    state = SessionState::Connected;
    info!(?state, "connected to server");

    stream.set_nodelay(true)?;
    prepare_linux_vhci_attach_dry_run(&vhci_adapter, &stream, vhci_device_id);

    let mut framed = Framed::new(stream, LengthDelimitedCodec::new());
    let (tx_sender, mut tx_receiver) = tokio::sync::mpsc::channel::<Frame>(100);
    let attach_request = Frame::attach_request(1);

    info!(
        request_id = attach_request.request_id,
        "sending attach request"
    );
    framed.send(Bytes::from(attach_request.encode())).await?;
    let attach_descriptors =
        wait_for_lifecycle_response(&mut framed, FrameType::AttachResponse).await?;
    if let Some(descriptors) = &attach_descriptors {
        info!(
            vendor_id = format_args!("{:04x}", descriptors.device.vendor_id),
            product_id = format_args!("{:04x}", descriptors.device.product_id),
            configurations = descriptors.configurations.len(),
            "received attach descriptors"
        );
    } else {
        warn!("attach response did not include descriptors");
    }
    let mut next_request_id = 2u64;
    run_mock_enumeration(
        &mut framed,
        &mut next_request_id,
        attach_descriptors.as_ref(),
    )
    .await?;

    state = SessionState::Attached;
    info!(?state, "mock VHCI session attached");

    let mock_local_usb_task = tokio::spawn(async move {
        let mut request_id = next_request_id;
        let mut sent_frames = 0u64;
        loop {
            if mock_frame_limit.is_some_and(|limit| sent_frames >= limit) {
                break;
            }

            sleep(mock_frame_interval).await;
            let frame = match mock_bulk_payload_len {
                Some(payload_len) => Frame::mock_bulk_request(request_id, payload_len),
                None => Frame::mock_request(request_id, b"Mock URB from Linux Client".to_vec()),
            };
            request_id += 1;
            sent_frames += 1;

            if tx_sender.send(frame).await.is_err() {
                break;
            }
        }
    });

    let mut data_responses = 0u64;
    let mut detach_sent = false;
    let mut pending_bulk_payloads = HashMap::new();
    let detach_request_id = mock_frame_limit
        .map(|limit| next_request_id + limit)
        .unwrap_or(u64::MAX);

    loop {
        tokio::select! {
            result = framed.next() => {
                match result {
                    Some(Ok(bytes)) => {
                        match handle_network_frame(&vhci_adapter, &mut pending_bulk_payloads, &bytes).await {
                            NetworkFrameAction::DataResponse => {
                                data_responses += 1;
                                if !detach_sent && mock_frame_limit == Some(data_responses) {
                                    let frame = Frame::detach_request(detach_request_id);
                                    info!(request_id = frame.request_id, "sending detach request");
                                    if let Err(e) = framed.send(Bytes::from(frame.encode())).await {
                                        error!(error = %e, "network send error");
                                        break;
                                    }
                                    detach_sent = true;
                                }
                            }
                            NetworkFrameAction::DetachAccepted => {
                                break;
                            }
                            NetworkFrameAction::None => {}
                        }
                    }
                    Some(Err(e)) => {
                        error!(error = %e, "network read error");
                        break;
                    }
                    None => {
                        info!("server disconnected");
                        break;
                    }
                }
            }

            Some(frame) = tx_receiver.recv() => {
                info!(
                    request_id = frame.request_id,
                    payload_len = frame.payload.len(),
                    "sending frame to server"
                );
                if let Ok(payload) = MockBulkPayload::decode(&frame.payload) {
                    info!(
                        request_id = frame.request_id,
                        bulk_len = payload.data.len(),
                        checksum = payload.checksum(),
                        "queued mock bulk payload for validation"
                    );
                    pending_bulk_payloads.insert(frame.request_id, payload);
                }

                if let Err(e) = framed.send(Bytes::from(frame.encode())).await {
                    error!(error = %e, "network send error");
                    break;
                }
            }

            Some(mock_processed_urb) = vhci_receiver.recv() => {
                info!(
                    urb_len = mock_processed_urb.len(),
                    "mock VHCI processed URB"
                );
            }
        }
    }

    state = SessionState::Detaching;
    info!(?state, "detaching mock VHCI session");
    mock_local_usb_task.abort();
    state = SessionState::Closed;
    info!(?state, "client session closed");
    Ok(())
}

#[cfg(unix)]
fn prepare_linux_vhci_attach_dry_run(
    vhci_adapter: &VhciAdapter,
    stream: &TcpStream,
    vhci_device_id: u32,
) {
    vhci_adapter.prepare_linux_attach_dry_run(stream.as_raw_fd(), vhci_device_id);
}

#[cfg(not(unix))]
fn prepare_linux_vhci_attach_dry_run(
    _vhci_adapter: &VhciAdapter,
    _stream: &TcpStream,
    _vhci_device_id: u32,
) {
}

async fn wait_for_lifecycle_response(
    framed: &mut Framed<TcpStream, LengthDelimitedCodec>,
    expected_frame_type: FrameType,
) -> Result<Option<UsbDescriptorSet>, Box<dyn std::error::Error>> {
    loop {
        let Some(result) = framed.next().await else {
            return Err("server disconnected during lifecycle handshake".into());
        };

        let bytes = result?;
        let frame = Frame::decode(&bytes)?;
        info!(
            request_id = frame.request_id,
            frame_type = ?frame.frame_type,
            status = frame.status,
            "received lifecycle frame"
        );

        if frame.frame_type != expected_frame_type {
            warn!(
                frame_type = ?frame.frame_type,
                expected_frame_type = ?expected_frame_type,
                "ignoring unexpected lifecycle frame"
            );
            continue;
        }

        if frame.status != 0 {
            return Err(format!("lifecycle request failed with status {}", frame.status).into());
        }

        let descriptors =
            if frame.frame_type == FrameType::AttachResponse && !frame.payload.is_empty() {
                Some(UsbDescriptorSet::decode(&frame.payload)?)
            } else {
                None
            };

        return Ok(descriptors);
    }
}

async fn run_mock_enumeration(
    framed: &mut Framed<TcpStream, LengthDelimitedCodec>,
    next_request_id: &mut u64,
    descriptors: Option<&UsbDescriptorSet>,
) -> Result<(), Box<dyn std::error::Error>> {
    send_control_request(
        framed,
        next_request_id,
        UsbControlSetup::get_descriptor(UsbDescriptorType::Device, 0, 18),
    )
    .await?;
    send_control_request(
        framed,
        next_request_id,
        UsbControlSetup::get_descriptor(UsbDescriptorType::Configuration, 0, 255),
    )
    .await?;
    let expected_report_descriptor = descriptors
        .map(|descriptors| descriptors.report_descriptor_bytes(0, 0, 0))
        .transpose()?;
    let report_descriptor_len = expected_report_descriptor
        .as_ref()
        .map(|descriptor| descriptor.len() as u16)
        .unwrap_or(255);
    let report_descriptor = send_control_request(
        framed,
        next_request_id,
        UsbControlSetup::get_interface_descriptor(
            UsbDescriptorType::Report,
            0,
            0,
            report_descriptor_len,
        ),
    )
    .await?;
    if let Some(expected_report_descriptor) = expected_report_descriptor {
        if report_descriptor.data != expected_report_descriptor {
            return Err(
                "HID report descriptor response did not match attach descriptor set".into(),
            );
        }
    }
    info!(
        report_descriptor_len = report_descriptor.data.len(),
        "mock HID report descriptor validated"
    );
    send_control_request(framed, next_request_id, UsbControlSetup::set_address(1)).await?;
    send_control_request(
        framed,
        next_request_id,
        UsbControlSetup::set_configuration(1),
    )
    .await?;

    info!("mock USB enumeration completed");
    Ok(())
}

async fn send_control_request(
    framed: &mut Framed<TcpStream, LengthDelimitedCodec>,
    next_request_id: &mut u64,
    setup: UsbControlSetup,
) -> Result<UsbControlTransfer, Box<dyn std::error::Error>> {
    let request_id = *next_request_id;
    *next_request_id += 1;
    let standard_request = setup.standard_request()?;
    let descriptor_type = if standard_request == UsbStandardRequest::GetDescriptor {
        Some(setup.descriptor_type()?)
    } else {
        None
    };
    let frame = Frame::control_request(request_id, setup);

    info!(
        request_id,
        standard_request = ?standard_request,
        descriptor_type = ?descriptor_type,
        "sending control request"
    );
    framed.send(Bytes::from(frame.encode())).await?;

    loop {
        let Some(result) = framed.next().await else {
            return Err("server disconnected during control transfer".into());
        };

        let bytes = result?;
        let response = Frame::decode(&bytes)?;
        info!(
            request_id = response.request_id,
            frame_type = ?response.frame_type,
            transfer_type = ?response.transfer_type,
            status = response.status,
            payload_len = response.payload.len(),
            "received control response"
        );

        if response.request_id != request_id || response.frame_type != FrameType::Response {
            warn!(
                expected_request_id = request_id,
                actual_request_id = response.request_id,
                frame_type = ?response.frame_type,
                "ignoring unrelated frame during control transfer"
            );
            continue;
        }

        if response.status != 0 {
            return Err(format!(
                "control request {:?} failed with status {}",
                standard_request, response.status
            )
            .into());
        }

        let control = UsbControlTransfer::decode(&response.payload)?;
        if standard_request == UsbStandardRequest::GetDescriptor {
            info!(
                descriptor_type = ?descriptor_type,
                response_len = control.data.len(),
                "completed GET_DESCRIPTOR"
            );
        } else {
            info!(standard_request = ?standard_request, "completed control request");
        }

        return Ok(control);
    }
}

async fn connect_with_retry(server_addr: SocketAddr) -> TcpStream {
    loop {
        match TcpStream::connect(server_addr).await {
            Ok(stream) => break stream,
            Err(e) => {
                warn!(error = %e, "connection failed; retrying in 2 seconds");
                sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NetworkFrameAction {
    None,
    DataResponse,
    DetachAccepted,
}

async fn handle_network_frame(
    vhci_adapter: &VhciAdapter,
    pending_bulk_payloads: &mut HashMap<u64, MockBulkPayload>,
    bytes: &[u8],
) -> NetworkFrameAction {
    match Frame::decode(bytes) {
        Ok(frame) => {
            info!(
                request_id = frame.request_id,
                frame_type = ?frame.frame_type,
                status = frame.status,
                payload_len = frame.payload.len(),
                "received network frame"
            );

            match frame.frame_type {
                FrameType::Response => {
                    if frame.status != 0 {
                        let status = frame.transfer_status().unwrap_or(TransferStatus::Failed);
                        warn!(
                            transfer_status = ?status,
                            "received failed transfer response"
                        );
                        pending_bulk_payloads.remove(&frame.request_id);
                        return NetworkFrameAction::DataResponse;
                    }

                    if let Some(expected) = pending_bulk_payloads.remove(&frame.request_id) {
                        match MockBulkPayload::decode(&frame.payload) {
                            Ok(actual) if actual == expected => {
                                info!(
                                    request_id = frame.request_id,
                                    bulk_len = actual.data.len(),
                                    checksum = actual.checksum(),
                                    "mock bulk transfer validated"
                                );
                            }
                            Ok(actual) => {
                                warn!(
                                    request_id = frame.request_id,
                                    expected_bulk_len = expected.data.len(),
                                    actual_bulk_len = actual.data.len(),
                                    "mock bulk transfer payload mismatch"
                                );
                            }
                            Err(e) => {
                                warn!(
                                    request_id = frame.request_id,
                                    error = %e,
                                    "mock bulk transfer response failed validation"
                                );
                            }
                        }
                    }

                    if let Err(e) = vhci_adapter.inject_urb(Bytes::from(frame.payload)).await {
                        warn!(error = %e, "VHCI inject error");
                        return NetworkFrameAction::None;
                    }

                    NetworkFrameAction::DataResponse
                }
                FrameType::DetachResponse => {
                    if frame.status != 0 {
                        warn!(status = frame.status, "detach request failed");
                        return NetworkFrameAction::None;
                    }

                    NetworkFrameAction::DetachAccepted
                }
                FrameType::CancelResponse => {
                    let status = frame.transfer_status().unwrap_or(TransferStatus::Failed);
                    info!(transfer_status = ?status, "received cancel response");
                    NetworkFrameAction::DataResponse
                }
                FrameType::ResetResponse => {
                    let status = frame.transfer_status().unwrap_or(TransferStatus::Failed);
                    info!(transfer_status = ?status, "received reset response");
                    NetworkFrameAction::None
                }
                FrameType::Event
                    if frame.transfer_type == TransferType::Interrupt && frame.endpoint == 0x81 =>
                {
                    match HidKeyboardInputReport::decode(&frame.payload) {
                        Ok(report) => {
                            info!(
                                modifiers = report.modifiers,
                                keycodes = ?report.keycodes,
                                "mock HID keyboard input report"
                            );
                        }
                        Err(e) => {
                            warn!(error = %e, "dropped malformed HID keyboard input report");
                        }
                    }
                    NetworkFrameAction::None
                }
                _ => {
                    warn!(frame_type = ?frame.frame_type, "expected response frame");
                    NetworkFrameAction::None
                }
            }
        }
        Err(e) => {
            warn!(error = %e, "dropped malformed network frame");
            NetworkFrameAction::None
        }
    }
}
