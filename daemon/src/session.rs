use crate::config::{MockTransferOutcome, MockTransferOutcomeRule};
use crate::driver_backend::{DeviceAttachRequest, DriverBackend};
use crate::endpoint_queue::{EndpointKey, EndpointTransferQueues};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use protocol::{
    Frame, FrameType, HidKeyboardInputReport, MockBulkPayload, TransferStatus, TransferType,
    UsbControlTransfer, UsbDescriptorSet, UsbDescriptorType, UsbStandardRequest,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionState {
    Connected,
    Attached,
    Detaching,
    Closed,
}

pub async fn run_single_client(
    stream: TcpStream,
    driver: Arc<dyn DriverBackend>,
    attach_device: Option<DeviceAttachRequest>,
    mock_hid_keycodes: Vec<u8>,
    mock_transfer_outcomes: Vec<MockTransferOutcomeRule>,
) -> Result<(), Box<dyn std::error::Error>> {
    stream.set_nodelay(true)?;
    let mut state = SessionState::Connected;
    info!(?state, "client session opened");

    let mut framed = Framed::new(stream, LengthDelimitedCodec::new());
    let (tx_sender, mut tx_receiver) = tokio::sync::mpsc::channel::<Bytes>(100);
    let mut attached_by_control_plane = false;
    let mut active_descriptors: Option<UsbDescriptorSet> = None;
    let mut assigned_address = 0u8;
    let mut active_configuration = 0u8;
    let mut endpoint_queues = EndpointTransferQueues::default();

    let tx_driver = Arc::clone(&driver);
    let tx_cancelled = Arc::new(AtomicBool::new(false));
    let tx_worker_cancelled = Arc::clone(&tx_cancelled);
    let tx_polling_task = tokio::task::spawn_blocking(move || loop {
        if tx_worker_cancelled.load(Ordering::Acquire) {
            break;
        }

        match tx_driver.poll_tx_frame() {
            Ok(Some(frame)) => {
                let outbound = outbound_network_frame(frame);
                if tx_sender
                    .blocking_send(Bytes::from(outbound.encode()))
                    .is_err()
                {
                    break;
                }
            }
            Ok(None) => {
                if let Err(e) = tx_driver.wait_for_tx_frame(Duration::from_millis(10)) {
                    warn!(error = %e, "failed while waiting for driver TX frame");
                }
            }
            Err(e) => {
                warn!(error = %e, "dropping frame from driver");
            }
        }
    });

    loop {
        tokio::select! {
            result = framed.next() => {
                match result {
                    Some(Ok(bytes)) => {
                        match Frame::decode(&bytes) {
                            Ok(frame) => {
                                info!(
                                    request_id = frame.request_id,
                                    frame_type = ?frame.frame_type,
                                    payload_len = frame.payload.len(),
                                    "received network frame"
                                );

                                match frame.frame_type {
                                    FrameType::AttachRequest => {
                                        let status = attach_session(
                                            &driver,
                                            attach_device,
                                            &mut attached_by_control_plane,
                                            &mut state,
                                        );
                                        let response = if status == 0 {
                                            let descriptors = descriptor_set_for_attach(attach_device);
                                            active_descriptors = Some(descriptors.clone());
                                            info!(
                                                vendor_id = format_args!("{:04x}", descriptors.device.vendor_id),
                                                product_id = format_args!("{:04x}", descriptors.device.product_id),
                                                configurations = descriptors.configurations.len(),
                                                "sending attach descriptors"
                                            );
                                            match frame.attach_response_with_descriptors(status, &descriptors) {
                                                Ok(response) => response,
                                                Err(e) => {
                                                    warn!(error = %e, "failed to encode attach descriptors");
                                                    frame.attach_response(-1)
                                                }
                                            }
                                        } else {
                                            frame.attach_response(status)
                                        };
                                        if let Err(e) = framed.send(Bytes::from(response.encode())).await {
                                            error!(error = %e, "network send error");
                                            break;
                                        }
                                        continue;
                                    }
                                    FrameType::DetachRequest => {
                                        let status = detach_session(
                                            &driver,
                                            &mut attached_by_control_plane,
                                            &mut state,
                                        );
                                        let response = frame.detach_response(status);
                                        if let Err(e) = framed.send(Bytes::from(response.encode())).await {
                                            error!(error = %e, "network send error");
                                        }
                                        break;
                                    }
                                    FrameType::CancelRequest => {
                                        let response = handle_cancel_request(&frame, state, &mut endpoint_queues);
                                        if let Err(e) = framed.send(Bytes::from(response.encode())).await {
                                            error!(error = %e, "network send error");
                                            break;
                                        }
                                        continue;
                                    }
                                    FrameType::ResetRequest => {
                                        let responses = handle_reset_request(&frame, state, &mut endpoint_queues);
                                        for response in responses {
                                            if let Err(e) = framed.send(Bytes::from(response.encode())).await {
                                                error!(error = %e, "network send error");
                                                break;
                                            }
                                        }
                                        continue;
                                    }
                                    FrameType::Request => {
                                        if state != SessionState::Attached {
                                            warn!(?state, "dropped data frame before attach");
                                            let response = frame.status_response(TransferStatus::Failed);
                                            if let Err(e) = framed.send(Bytes::from(response.encode())).await {
                                                error!(error = %e, "network send error");
                                                break;
                                            }
                                            continue;
                                        }

                                        if frame.transfer_type == TransferType::Control && frame.endpoint == 0 {
                                            let control_response = handle_control_request(
                                                &frame,
                                                active_descriptors.as_ref(),
                                                &mut assigned_address,
                                                &mut active_configuration,
                                                &mock_hid_keycodes,
                                            );
                                            if let Err(e) = framed.send(Bytes::from(control_response.response.encode())).await {
                                                error!(error = %e, "network send error");
                                                break;
                                            }
                                            for event in control_response.events {
                                                if let Err(e) = framed.send(Bytes::from(event.encode())).await {
                                                    error!(error = %e, "network send error");
                                                    break;
                                                }
                                            }
                                            continue;
                                        }

                                        if let Some(response) = mock_transfer_outcome_response(&frame, &mock_transfer_outcomes) {
                                            if let Err(e) = framed.send(Bytes::from(response.encode())).await {
                                                error!(error = %e, "network send error");
                                                break;
                                            }
                                            continue;
                                        }

                                        if let Err(e) = endpoint_queues.enqueue(frame) {
                                            warn!(error = %e, "failed to queue endpoint transfer");
                                            continue;
                                        }

                                        info!(
                                            queued_transfers = endpoint_queues.len(),
                                            "queued endpoint transfer"
                                        );
                                        for response in flush_endpoint_transfers(&driver, &mut endpoint_queues) {
                                            if let Err(e) = framed.send(Bytes::from(response.encode())).await {
                                                error!(error = %e, "network send error");
                                                break;
                                            }
                                        }
                                        continue;
                                    }
                                    _ => {
                                        warn!(
                                            frame_type = ?frame.frame_type,
                                            "unexpected frame type from client"
                                        );
                                        continue;
                                    }
                                }
                            }
                            Err(e) => {
                                warn!(error = %e, "dropped malformed network frame");
                                continue;
                            }
                        }
                    }
                    Some(Err(e)) => {
                        error!(error = %e, "network read error");
                        break;
                    }
                    None => {
                        info!("client disconnected");
                        break;
                    }
                }
            }

            Some(bytes) = tx_receiver.recv() => {
                if let Err(e) = framed.send(bytes.into()).await {
                    error!(error = %e, "network send error");
                    break;
                }
            }
        }
    }

    state = SessionState::Detaching;
    debug!(?state, "detaching mock session");
    detach_session(&driver, &mut attached_by_control_plane, &mut state);
    clear_endpoint_transfers(&mut endpoint_queues);
    tx_cancelled.store(true, Ordering::Release);
    if let Err(e) = tx_polling_task.await {
        warn!(error = %e, "driver TX worker failed to join");
    }
    state = SessionState::Closed;
    info!(?state, "client session closed");
    Ok(())
}

fn outbound_network_frame(frame: Frame) -> Frame {
    if frame.frame_type == FrameType::Request {
        match MockBulkPayload::decode(&frame.payload) {
            Ok(payload) => {
                info!(
                    request_id = frame.request_id,
                    bulk_len = payload.data.len(),
                    checksum = payload.checksum(),
                    "mock bulk transfer echoed"
                );
                frame.mock_response(payload.encode())
            }
            Err(_) => frame.mock_response(b"Mock response from daemon driver".to_vec()),
        }
    } else {
        frame
    }
}

fn mock_transfer_outcome_response(
    frame: &Frame,
    outcomes: &[MockTransferOutcomeRule],
) -> Option<Frame> {
    let rule = outcomes
        .iter()
        .find(|rule| rule.request_id == frame.request_id)?;

    match rule.outcome {
        MockTransferOutcome::Status(status) => {
            info!(
                request_id = frame.request_id,
                transfer_status = ?status,
                "injecting mock transfer status"
            );
            Some(frame.status_response(status))
        }
        MockTransferOutcome::ShortPacket(len) => {
            let mut payload = b"Mock response from daemon driver".to_vec();
            payload.truncate(len);
            info!(
                request_id = frame.request_id,
                requested_payload_len = len,
                actual_payload_len = payload.len(),
                "injecting mock short packet"
            );
            Some(frame.mock_response(payload))
        }
    }
}

pub(crate) fn attach_session(
    driver: &Arc<dyn DriverBackend>,
    attach_device: Option<DeviceAttachRequest>,
    attached_by_control_plane: &mut bool,
    state: &mut SessionState,
) -> i32 {
    if *state == SessionState::Attached {
        return 0;
    }

    if let Some(request) = attach_device {
        if let Err(e) = driver.attach_device(request) {
            warn!(error = %e, "failed to attach device through driver control plane");
            return -1;
        }

        *attached_by_control_plane = true;
        info!(
            vendor_id = format_args!("{:04x}", request.vendor_id),
            product_id = format_args!("{:04x}", request.product_id),
            bus_id = request.bus_id,
            port_id = request.port_id,
            "device attached through driver control plane"
        );
    }

    *state = SessionState::Attached;
    info!(?state, "protocol session attached");
    0
}

pub(crate) fn detach_session(
    driver: &Arc<dyn DriverBackend>,
    attached_by_control_plane: &mut bool,
    state: &mut SessionState,
) -> i32 {
    if *state == SessionState::Closed {
        return 0;
    }

    *state = SessionState::Detaching;

    if *attached_by_control_plane {
        if let Err(e) = driver.detach_device(0) {
            warn!(error = %e, "failed to detach device through driver control plane");
            return -1;
        }

        *attached_by_control_plane = false;
        info!("device detached through driver control plane");
    }

    *state = SessionState::Closed;
    info!(?state, "protocol session detached");
    0
}

fn clear_endpoint_transfers(endpoint_queues: &mut EndpointTransferQueues) {
    let dropped_transfers = endpoint_queues.clear();
    if dropped_transfers == 0 {
        info!("endpoint transfer queues clean at session cleanup");
        return;
    }

    warn!(
        dropped_transfers,
        "dropped queued endpoint transfers during session cleanup"
    );
}

fn handle_cancel_request(
    frame: &Frame,
    state: SessionState,
    endpoint_queues: &mut EndpointTransferQueues,
) -> Frame {
    if state != SessionState::Attached {
        warn!(
            ?state,
            request_id = frame.request_id,
            "cancel before attach"
        );
        return frame.cancel_response(TransferStatus::Failed);
    }

    match endpoint_queues.cancel(frame.request_id) {
        Some(cancelled) => {
            info!(
                request_id = cancelled.frame.request_id,
                endpoint = cancelled.key.endpoint,
                transfer_type = ?cancelled.key.transfer_type,
                sequence = cancelled.sequence,
                "cancelled queued endpoint transfer"
            );
            frame.cancel_response(TransferStatus::Cancelled)
        }
        None => {
            warn!(
                request_id = frame.request_id,
                endpoint = frame.endpoint,
                transfer_type = ?frame.transfer_type,
                "cancel request did not match a queued transfer"
            );
            frame.cancel_response(TransferStatus::Failed)
        }
    }
}

fn handle_reset_request(
    frame: &Frame,
    state: SessionState,
    endpoint_queues: &mut EndpointTransferQueues,
) -> Vec<Frame> {
    if state != SessionState::Attached {
        warn!(?state, request_id = frame.request_id, "reset before attach");
        return vec![frame.reset_response(TransferStatus::Failed)];
    }

    let key = EndpointKey::from_frame(frame);
    let dropped = endpoint_queues.reset_endpoint(key);
    info!(
        endpoint = key.endpoint,
        transfer_type = ?key.transfer_type,
        dropped_transfers = dropped.len(),
        "reset endpoint transfer queue"
    );

    let mut responses = Vec::with_capacity(dropped.len() + 1);
    responses.push(frame.reset_response(TransferStatus::Ok));
    responses.extend(
        dropped
            .into_iter()
            .map(|transfer| transfer.frame.status_response(TransferStatus::Reset)),
    );
    responses
}

pub(crate) fn descriptor_set_for_attach(
    attach_device: Option<DeviceAttachRequest>,
) -> UsbDescriptorSet {
    let (vendor_id, product_id) = attach_device
        .map(|request| (request.vendor_id, request.product_id))
        .unwrap_or((0x1209, 0x0001));

    UsbDescriptorSet::mock_hid_keyboard(vendor_id, product_id)
}

fn flush_endpoint_transfers(
    driver: &Arc<dyn DriverBackend>,
    endpoint_queues: &mut EndpointTransferQueues,
) -> Vec<Frame> {
    let mut failed_responses = Vec::new();

    while let Some(transfer) = endpoint_queues.pop_next() {
        let bytes = transfer.frame.encode();
        info!(
            request_id = transfer.frame.request_id,
            endpoint = transfer.key.endpoint,
            transfer_type = ?transfer.key.transfer_type,
            sequence = transfer.sequence,
            "dispatching endpoint transfer"
        );

        if let Err(e) = driver.push_rx_bytes(&bytes) {
            warn!(error = %e, "failed to push queued transfer to driver");
            failed_responses.push(transfer.frame.status_response(TransferStatus::Failed));
            continue;
        }

        if !driver.pump_once() {
            warn!(
                request_id = transfer.frame.request_id,
                "driver did not complete queued transfer immediately"
            );
        }
    }

    failed_responses
}

pub(crate) fn handle_control_request(
    frame: &Frame,
    descriptors: Option<&UsbDescriptorSet>,
    assigned_address: &mut u8,
    active_configuration: &mut u8,
    mock_hid_keycodes: &[u8],
) -> ControlResponse {
    let Ok(control) = UsbControlTransfer::decode(&frame.payload) else {
        warn!("dropped malformed control transfer");
        return ControlResponse::response(
            frame.control_response(TransferStatus::Failed as i32, Vec::new()),
        );
    };

    match control.setup.standard_request() {
        Ok(UsbStandardRequest::GetDescriptor) => {
            let descriptor_type = match control.setup.descriptor_type() {
                Ok(descriptor_type) => descriptor_type,
                Err(e) => {
                    warn!(error = %e, "unsupported descriptor request");
                    return ControlResponse::response(
                        frame.control_response(TransferStatus::Stall as i32, Vec::new()),
                    );
                }
            };
            let Some(descriptors) = descriptors else {
                warn!("descriptor request before descriptor set is available");
                return ControlResponse::response(
                    frame.control_response(TransferStatus::Failed as i32, Vec::new()),
                );
            };

            let descriptor = match descriptor_type {
                UsbDescriptorType::Device => descriptors.device_descriptor_bytes(),
                UsbDescriptorType::Configuration => {
                    descriptors.configuration_descriptor_bytes(control.setup.descriptor_index())
                }
                UsbDescriptorType::Report => descriptors.report_descriptor_bytes(
                    0,
                    control.setup.index as u8,
                    control.setup.descriptor_index(),
                ),
                _ => {
                    warn!(descriptor_type = ?descriptor_type, "unsupported descriptor type");
                    return ControlResponse::response(
                        frame.control_response(TransferStatus::Stall as i32, Vec::new()),
                    );
                }
            };

            match descriptor {
                Ok(mut bytes) => {
                    bytes.truncate(control.setup.length as usize);
                    info!(
                        descriptor_type = ?descriptor_type,
                        descriptor_index = control.setup.descriptor_index(),
                        response_len = bytes.len(),
                        "handled GET_DESCRIPTOR"
                    );
                    ControlResponse::response(
                        frame.control_response(TransferStatus::Ok as i32, bytes),
                    )
                }
                Err(e) => {
                    warn!(error = %e, "failed to build descriptor response");
                    ControlResponse::response(
                        frame.control_response(TransferStatus::Stall as i32, Vec::new()),
                    )
                }
            }
        }
        Ok(UsbStandardRequest::SetAddress) => {
            let address = control.setup.value as u8;
            if control.setup.value > 127 || control.setup.length != 0 {
                warn!(value = control.setup.value, "invalid SET_ADDRESS");
                return ControlResponse::response(
                    frame.control_response(TransferStatus::Stall as i32, Vec::new()),
                );
            }

            *assigned_address = address;
            info!(address = *assigned_address, "handled SET_ADDRESS");
            ControlResponse::response(frame.control_response(TransferStatus::Ok as i32, Vec::new()))
        }
        Ok(UsbStandardRequest::SetConfiguration) => {
            let configuration = control.setup.value as u8;
            let supported = configuration == 0
                || descriptors
                    .map(|descriptors| {
                        descriptors.configurations.iter().any(|candidate| {
                            candidate.descriptor.configuration_value == configuration
                        })
                    })
                    .unwrap_or(false);
            if control.setup.value > u8::MAX as u16 || control.setup.length != 0 || !supported {
                warn!(value = control.setup.value, "invalid SET_CONFIGURATION");
                return ControlResponse::response(
                    frame.control_response(TransferStatus::Stall as i32, Vec::new()),
                );
            }

            *active_configuration = configuration;
            info!(
                configuration = *active_configuration,
                "handled SET_CONFIGURATION"
            );
            let mut response = ControlResponse::response(
                frame.control_response(TransferStatus::Ok as i32, Vec::new()),
            );
            if configuration != 0 {
                response.events = mock_hid_keyboard_events(mock_hid_keycodes);
                info!(
                    events = response.events.len(),
                    "queued mock HID input reports"
                );
            }
            response
        }
        Err(e) => {
            warn!(error = %e, request = control.setup.request, "unsupported control request");
            ControlResponse::response(
                frame.control_response(TransferStatus::Stall as i32, Vec::new()),
            )
        }
    }
}

pub(crate) struct ControlResponse {
    pub(crate) response: Frame,
    pub(crate) events: Vec<Frame>,
}

impl ControlResponse {
    fn response(response: Frame) -> Self {
        Self {
            response,
            events: Vec::new(),
        }
    }
}

fn mock_hid_keyboard_events(keycodes: &[u8]) -> Vec<Frame> {
    let mut events = Vec::with_capacity(keycodes.len().saturating_mul(2));

    for &keycode in keycodes {
        events.push(Frame::hid_keyboard_input_event(
            0,
            HidKeyboardInputReport::key_press(keycode),
        ));
        events.push(Frame::hid_keyboard_input_event(
            0,
            HidKeyboardInputReport::empty(),
        ));
    }

    events
}
