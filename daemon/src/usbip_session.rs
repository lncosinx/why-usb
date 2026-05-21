use crate::driver_backend::{DeviceAttachRequest, DriverBackend};
use crate::session::{self, SessionState};
use protocol::{
    Direction, Frame, FrameType, TransferStatus, TransferType, UsbControlSetup, UsbControlTransfer,
    UsbIpCmdSubmit, UsbIpCmdUnlink, UsbIpCommand, UsbIpHeaderBasic, UsbIpRetSubmit, UsbIpRetUnlink,
    USBIP_HEADER_LEN,
};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, error, info, warn};

pub async fn run_single_usbip_client(
    mut stream: TcpStream,
    driver: Arc<dyn DriverBackend>,
    attach_device: Option<DeviceAttachRequest>,
    mock_hid_keycodes: Vec<u8>,
) -> Result<(), Box<dyn std::error::Error>> {
    stream.set_nodelay(true)?;

    let mut state = SessionState::Connected;
    let mut attached_by_control_plane = false;
    let attach_status = session::attach_session(
        &driver,
        attach_device,
        &mut attached_by_control_plane,
        &mut state,
    );
    if attach_status != 0 {
        return Err("failed to attach USB/IP mock device".into());
    }

    let descriptors = session::descriptor_set_for_attach(attach_device);
    let mut assigned_address = 0u8;
    let mut active_configuration = 0u8;
    let mut pending_interrupt_reports = VecDeque::<Vec<u8>>::new();

    info!(
        vendor_id = format_args!("{:04x}", descriptors.device.vendor_id),
        product_id = format_args!("{:04x}", descriptors.device.product_id),
        "USB/IP mock session attached"
    );

    loop {
        let packet = match read_usbip_packet(&mut stream).await {
            Ok(Some(packet)) => packet,
            Ok(None) => {
                info!("USB/IP peer disconnected");
                break;
            }
            Err(e) => {
                warn!(error = %e, "failed to read USB/IP packet");
                break;
            }
        };

        let header = match UsbIpHeaderBasic::decode(&packet[..UsbIpHeaderBasic::LEN]) {
            Ok(header) => header,
            Err(e) => {
                warn!(error = %e, "dropped malformed USB/IP header");
                continue;
            }
        };

        match header.command {
            UsbIpCommand::CmdSubmit => {
                let submit = match UsbIpCmdSubmit::decode(&packet) {
                    Ok(submit) => submit,
                    Err(e) => {
                        warn!(error = %e, "dropped malformed USB/IP submit");
                        continue;
                    }
                };
                let response = handle_submit(
                    submit,
                    &descriptors,
                    &mut assigned_address,
                    &mut active_configuration,
                    &mock_hid_keycodes,
                    &mut pending_interrupt_reports,
                );

                if let Err(e) = stream.write_all(&response.encode()?).await {
                    error!(error = %e, "failed to write USB/IP submit response");
                    break;
                }
            }
            UsbIpCommand::CmdUnlink => {
                let unlink = match UsbIpCmdUnlink::decode(&packet) {
                    Ok(unlink) => unlink,
                    Err(e) => {
                        warn!(error = %e, "dropped malformed USB/IP unlink");
                        continue;
                    }
                };
                let response = UsbIpRetUnlink::new(unlink.header.seqnum, 0);
                if let Err(e) = stream.write_all(&response.encode()).await {
                    error!(error = %e, "failed to write USB/IP unlink response");
                    break;
                }
            }
            other => {
                warn!(command = ?other, "unexpected USB/IP command from client");
            }
        }
    }

    state = SessionState::Detaching;
    debug!(?state, "detaching USB/IP mock session");
    session::detach_session(&driver, &mut attached_by_control_plane, &mut state);
    state = SessionState::Closed;
    info!(?state, "USB/IP mock session closed");
    Ok(())
}

async fn read_usbip_packet(stream: &mut TcpStream) -> Result<Option<Vec<u8>>, std::io::Error> {
    let mut header = [0u8; USBIP_HEADER_LEN];
    match stream.read_exact(&mut header).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }

    let basic = UsbIpHeaderBasic::decode(&header[..UsbIpHeaderBasic::LEN])
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let payload_len = match basic.command {
        UsbIpCommand::CmdSubmit if submit_is_out(&header) => submit_transfer_buffer_len(&header),
        _ => 0,
    };
    let mut packet = header.to_vec();
    if payload_len > 0 {
        let mut payload = vec![0u8; payload_len];
        stream.read_exact(&mut payload).await?;
        packet.extend_from_slice(&payload);
    }

    Ok(Some(packet))
}

fn submit_is_out(header: &[u8; USBIP_HEADER_LEN]) -> bool {
    u32::from_be_bytes(header[12..16].try_into().unwrap()) == 0
}

fn submit_transfer_buffer_len(header: &[u8; USBIP_HEADER_LEN]) -> usize {
    u32::from_be_bytes(header[24..28].try_into().unwrap()) as usize
}

fn handle_submit(
    submit: UsbIpCmdSubmit,
    descriptors: &protocol::UsbDescriptorSet,
    assigned_address: &mut u8,
    active_configuration: &mut u8,
    mock_hid_keycodes: &[u8],
    pending_interrupt_reports: &mut VecDeque<Vec<u8>>,
) -> UsbIpRetSubmit {
    if submit.header.endpoint == 0 {
        return handle_control_submit(
            submit,
            descriptors,
            assigned_address,
            active_configuration,
            mock_hid_keycodes,
            pending_interrupt_reports,
        );
    }

    if submit.header.endpoint == 0x81 {
        let report = pending_interrupt_reports.pop_front().unwrap_or_default();
        info!(
            seqnum = submit.header.seqnum,
            report_len = report.len(),
            "handled USB/IP HID interrupt IN submit"
        );
        return UsbIpRetSubmit::ok_for(&submit, report);
    }

    warn!(
        seqnum = submit.header.seqnum,
        endpoint = submit.header.endpoint,
        "USB/IP endpoint is not implemented in mock device"
    );
    failed_submit_response(&submit, TransferStatus::Stall)
}

fn handle_control_submit(
    submit: UsbIpCmdSubmit,
    descriptors: &protocol::UsbDescriptorSet,
    assigned_address: &mut u8,
    active_configuration: &mut u8,
    mock_hid_keycodes: &[u8],
    pending_interrupt_reports: &mut VecDeque<Vec<u8>>,
) -> UsbIpRetSubmit {
    let setup = match UsbControlSetup::decode(&submit.setup) {
        Ok(setup) => setup,
        Err(e) => {
            warn!(error = %e, "USB/IP control submit has invalid setup packet");
            return failed_submit_response(&submit, TransferStatus::Stall);
        }
    };
    let frame = Frame::new(
        u64::from(submit.header.seqnum),
        FrameType::Request,
        match setup.transfer_direction() {
            Direction::HostToDevice => Direction::HostToDevice,
            Direction::DeviceToHost => Direction::DeviceToHost,
        },
        TransferType::Control,
        0,
        0,
        UsbControlTransfer::new(setup, submit.transfer_buffer.clone()).encode(),
    );
    let control_response = session::handle_control_request(
        &frame,
        Some(descriptors),
        assigned_address,
        active_configuration,
        mock_hid_keycodes,
    );
    for event in control_response.events {
        if event.transfer_type == TransferType::Interrupt && event.endpoint == 0x81 {
            pending_interrupt_reports.push_back(event.payload);
        }
    }
    let payload = UsbControlTransfer::decode(&control_response.response.payload)
        .map(|transfer| transfer.data)
        .unwrap_or_default();

    let mut response = UsbIpRetSubmit::ok_for(&submit, payload);
    response.status = control_response.response.status;
    if response.status != 0 {
        response.transfer_buffer.clear();
        response.actual_length = 0;
    }
    response
}

fn failed_submit_response(submit: &UsbIpCmdSubmit, status: TransferStatus) -> UsbIpRetSubmit {
    let mut response = UsbIpRetSubmit::ok_for(submit, Vec::new());
    response.status = status as i32;
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::UsbIpDirection;

    #[test]
    fn handles_usbip_control_get_descriptor() {
        let descriptors = protocol::UsbDescriptorSet::mock_hid_keyboard(0x1209, 0x0001);
        let submit = UsbIpCmdSubmit::new(
            7,
            0x0001_0002,
            UsbIpDirection::In,
            0,
            Vec::new(),
            UsbControlSetup::get_descriptor(protocol::UsbDescriptorType::Device, 0, 18).encode(),
        );
        let mut assigned_address = 0;
        let mut active_configuration = 0;
        let mut reports = VecDeque::new();

        let response = handle_submit(
            submit,
            &descriptors,
            &mut assigned_address,
            &mut active_configuration,
            &[protocol::HidKeyboardInputReport::KEY_A],
            &mut reports,
        );

        assert_eq!(response.status, 0);
        assert_eq!(response.actual_length, 18);
        assert_eq!(response.transfer_buffer[0], 18);
        assert_eq!(response.transfer_buffer[1], 1);
    }

    #[test]
    fn queues_hid_report_after_set_configuration() {
        let descriptors = protocol::UsbDescriptorSet::mock_hid_keyboard(0x1209, 0x0001);
        let submit = UsbIpCmdSubmit::new(
            8,
            0x0001_0002,
            UsbIpDirection::Out,
            0,
            Vec::new(),
            UsbControlSetup::set_configuration(1).encode(),
        );
        let mut assigned_address = 0;
        let mut active_configuration = 0;
        let mut reports = VecDeque::new();

        let response = handle_submit(
            submit,
            &descriptors,
            &mut assigned_address,
            &mut active_configuration,
            &[protocol::HidKeyboardInputReport::KEY_A],
            &mut reports,
        );

        assert_eq!(response.status, 0);
        assert_eq!(active_configuration, 1);
        assert_eq!(reports.len(), 2);
    }
}
