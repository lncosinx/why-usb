use protocol::{Frame, ProtocolError};
use std::fmt;
#[cfg(windows)]
use std::io;
use std::time::Duration;

const MAX_DRIVER_FRAME_SIZE: usize = 1024 * 64;

#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("vhci.h");

        fn init_vhci_driver() -> i32;
        fn cleanup_vhci_driver();

        unsafe fn tx_ring_pop_frame(dst: *mut u8, max_len: usize, out_len: *mut usize) -> bool;
        unsafe fn rx_ring_push_frame(src: *const u8, len: usize) -> bool;
        fn mock_driver_pump_once() -> bool;
    }
}

pub trait DriverBackend: Send + Sync {
    fn attach_device(&self, request: DeviceAttachRequest) -> Result<(), DriverError>;
    fn detach_device(&self, reason: u32) -> Result<(), DriverError>;
    fn push_rx_bytes(&self, bytes: &[u8]) -> Result<(), DriverError>;
    fn pump_once(&self) -> bool;
    fn poll_tx_frame(&self) -> Result<Option<Frame>, DriverError>;
    fn wait_for_tx_frame(&self, timeout: Duration) -> Result<bool, DriverError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceAttachRequest {
    pub vendor_id: u16,
    pub product_id: u16,
    pub bus_id: u8,
    pub port_id: u8,
    pub flags: u16,
}

pub struct MockDriverBackend;

impl MockDriverBackend {
    pub fn init() -> Result<Self, DriverError> {
        let status = ffi::init_vhci_driver();
        if status != 0 {
            return Err(DriverError::InitFailed(status));
        }

        Ok(Self)
    }
}

impl DriverBackend for MockDriverBackend {
    fn attach_device(&self, _request: DeviceAttachRequest) -> Result<(), DriverError> {
        Ok(())
    }

    fn detach_device(&self, _reason: u32) -> Result<(), DriverError> {
        Ok(())
    }

    fn push_rx_bytes(&self, bytes: &[u8]) -> Result<(), DriverError> {
        let success = unsafe { ffi::rx_ring_push_frame(bytes.as_ptr(), bytes.len()) };
        if !success {
            return Err(DriverError::RxRingFull(bytes.len()));
        }

        Ok(())
    }

    fn pump_once(&self) -> bool {
        ffi::mock_driver_pump_once()
    }

    fn poll_tx_frame(&self) -> Result<Option<Frame>, DriverError> {
        let mut buffer = vec![0u8; MAX_DRIVER_FRAME_SIZE];
        let mut read_len = 0usize;
        let has_frame =
            unsafe { ffi::tx_ring_pop_frame(buffer.as_mut_ptr(), buffer.len(), &mut read_len) };

        if !has_frame {
            return Ok(None);
        }

        Frame::decode(&buffer[..read_len])
            .map(Some)
            .map_err(DriverError::MalformedTxFrame)
    }

    fn wait_for_tx_frame(&self, timeout: Duration) -> Result<bool, DriverError> {
        std::thread::sleep(timeout);
        Ok(false)
    }
}

impl Drop for MockDriverBackend {
    fn drop(&mut self) {
        ffi::cleanup_vhci_driver();
    }
}

#[derive(Debug)]
pub enum DriverError {
    InitFailed(i32),
    RxRingFull(usize),
    MalformedTxFrame(ProtocolError),
    #[cfg(windows)]
    MappedRing(String),
    #[cfg(windows)]
    UnsupportedBackend(String),
    #[cfg(windows)]
    WindowsIo {
        operation: &'static str,
        source: io::Error,
    },
    #[cfg(windows)]
    Abi(String),
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InitFailed(status) => write!(f, "driver initialization failed: {status}"),
            Self::RxRingFull(len) => write!(f, "RX ring is full; dropped {len} byte frame"),
            Self::MalformedTxFrame(err) => write!(f, "malformed TX frame from driver: {err}"),
            #[cfg(windows)]
            Self::MappedRing(message) => write!(f, "mapped ring error: {message}"),
            #[cfg(windows)]
            Self::UnsupportedBackend(message) => write!(f, "unsupported driver backend: {message}"),
            #[cfg(windows)]
            Self::WindowsIo { operation, source } => {
                write!(f, "Windows I/O failed during {operation}: {source}")
            }
            #[cfg(windows)]
            Self::Abi(message) => write!(f, "driver ABI error: {message}"),
        }
    }
}

impl std::error::Error for DriverError {}
