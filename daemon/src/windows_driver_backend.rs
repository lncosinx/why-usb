#![cfg(windows)]

use crate::driver_backend::{DeviceAttachRequest, DriverBackend, DriverError};
use crate::ioctl;
use crate::mapped_ring::MappedRing;
use protocol::Frame;
use std::ffi::{c_void, OsStr};
use std::io;
use std::mem::{size_of, MaybeUninit};
use std::os::windows::ffi::OsStrExt;
use std::ptr::null_mut;
use std::time::Duration;
use tracing::info;

type Dword = u32;
type Bool = i32;
type Handle = *mut c_void;

const GENERIC_READ: Dword = 0x8000_0000;
const GENERIC_WRITE: Dword = 0x4000_0000;
const FILE_SHARE_READ: Dword = 0x0000_0001;
const FILE_SHARE_WRITE: Dword = 0x0000_0002;
const OPEN_EXISTING: Dword = 3;
const FILE_ATTRIBUTE_NORMAL: Dword = 0x0000_0080;
const FILE_MAP_ALL_ACCESS: Dword = 0x000f_001f;
const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;
const WAIT_OBJECT_0: Dword = 0;
const WAIT_TIMEOUT: Dword = 0x0000_0102;
const WAIT_FAILED: Dword = 0xffff_ffff;

unsafe extern "system" {
    fn CreateFileW(
        lpFileName: *const u16,
        dwDesiredAccess: Dword,
        dwShareMode: Dword,
        lpSecurityAttributes: *mut c_void,
        dwCreationDisposition: Dword,
        dwFlagsAndAttributes: Dword,
        hTemplateFile: Handle,
    ) -> Handle;

    fn DeviceIoControl(
        hDevice: Handle,
        dwIoControlCode: Dword,
        lpInBuffer: *mut c_void,
        nInBufferSize: Dword,
        lpOutBuffer: *mut c_void,
        nOutBufferSize: Dword,
        lpBytesReturned: *mut Dword,
        lpOverlapped: *mut c_void,
    ) -> Bool;

    fn CloseHandle(hObject: Handle) -> Bool;

    fn MapViewOfFile(
        hFileMappingObject: Handle,
        dwDesiredAccess: Dword,
        dwFileOffsetHigh: Dword,
        dwFileOffsetLow: Dword,
        dwNumberOfBytesToMap: usize,
    ) -> *mut c_void;

    fn UnmapViewOfFile(lpBaseAddress: *const c_void) -> Bool;

    fn WaitForSingleObject(hHandle: Handle, dwMilliseconds: Dword) -> Dword;

    fn SetEvent(hEvent: Handle) -> Bool;
}

pub struct WindowsDriverBackend {
    device_path: String,
    handle: DeviceHandle,
    session_id: u64,
    max_frame_size: u32,
    shared_memory: Option<SharedMemoryMapping>,
}

impl WindowsDriverBackend {
    pub fn open(device_path: impl Into<String>) -> Result<Self, DriverError> {
        let device_path = device_path.into();
        let handle = DeviceHandle::open(&device_path)?;
        let response: ioctl::SessionOpenResponse = handle.ioctl(
            ioctl::IOCTL_WHY_USB_SESSION_OPEN,
            &ioctl::SessionOpenRequest::new(0),
        )?;

        validate_header(response.header, size_of::<ioctl::SessionOpenResponse>())?;

        let status = response.status;
        if status != ioctl::IoctlStatus::Ok as u32 {
            return Err(DriverError::Abi(format!(
                "SESSION_OPEN returned status {status}"
            )));
        }

        let session_id = response.session_id;
        let max_frame_size = response.max_frame_size;
        info!(session_id, max_frame_size, "opened Windows driver session");

        let backend = Self {
            device_path,
            handle,
            session_id,
            max_frame_size,
            shared_memory: None,
        };

        let status = backend.get_status()?;
        let session_state = status.session_state;
        info!(session_id, session_state, "queried Windows driver status");

        Ok(backend)
    }

    pub fn device_path(&self) -> &str {
        &self.device_path
    }

    pub fn session_id(&self) -> u64 {
        self.session_id
    }

    pub fn max_frame_size(&self) -> u32 {
        self.max_frame_size
    }

    pub fn map_shared_memory(&mut self) -> Result<(), DriverError> {
        if self.shared_memory.is_some() {
            return Ok(());
        }

        let info: ioctl::SharedMemoryInfo = self
            .handle
            .ioctl_no_input(ioctl::IOCTL_WHY_USB_GET_SHARED_MEMORY)?;
        validate_header(info.header, size_of::<ioctl::SharedMemoryInfo>())?;

        let mapping = SharedMemoryMapping::map(info)?;
        info!(
            mapping_size = mapping.mapping_size,
            tx_ring_offset = mapping.tx_ring_offset,
            rx_ring_offset = mapping.rx_ring_offset,
            "mapped Windows driver shared memory"
        );
        self.shared_memory = Some(mapping);
        Ok(())
    }

    fn get_status(&self) -> Result<ioctl::StatusResponse, DriverError> {
        let response: ioctl::StatusResponse = self
            .handle
            .ioctl_no_input(ioctl::IOCTL_WHY_USB_GET_STATUS)?;
        validate_header(response.header, size_of::<ioctl::StatusResponse>())?;
        Ok(response)
    }
}

struct SharedMemoryMapping {
    section: DeviceHandle,
    tx_event: Option<DeviceHandle>,
    rx_event: Option<DeviceHandle>,
    view: *mut c_void,
    mapping_size: u32,
    tx_ring_offset: u32,
    rx_ring_offset: u32,
}

unsafe impl Send for SharedMemoryMapping {}
unsafe impl Sync for SharedMemoryMapping {}

impl SharedMemoryMapping {
    fn map(info: ioctl::SharedMemoryInfo) -> Result<Self, DriverError> {
        let section_handle = info.section_handle;
        let tx_event_handle = info.tx_event_handle;
        let rx_event_handle = info.rx_event_handle;
        let mapping_size = info.mapping_size;
        let tx_ring_offset = info.tx_ring_offset;
        let rx_ring_offset = info.rx_ring_offset;

        if section_handle == 0 || mapping_size < size_of::<ioctl::SharedMemoryHeader>() as u32 {
            return Err(DriverError::Abi(
                "driver returned invalid shared memory handle or size".to_string(),
            ));
        }

        let view = unsafe {
            MapViewOfFile(
                section_handle as Handle,
                FILE_MAP_ALL_ACCESS,
                0,
                0,
                mapping_size as usize,
            )
        };

        if view.is_null() {
            return Err(last_windows_error("MapViewOfFile"));
        }

        let header = unsafe { *(view as *const ioctl::SharedMemoryHeader) };
        if let Err(e) = header.validate() {
            unsafe {
                UnmapViewOfFile(view);
            }
            return Err(DriverError::Abi(e.to_string()));
        }

        Ok(Self {
            section: DeviceHandle(section_handle as Handle),
            tx_event: non_null_handle(tx_event_handle).map(DeviceHandle),
            rx_event: non_null_handle(rx_event_handle).map(DeviceHandle),
            view,
            mapping_size,
            tx_ring_offset,
            rx_ring_offset,
        })
    }

    fn tx_ring(&self) -> Result<MappedRing, DriverError> {
        self.ring(self.tx_ring_offset, "TX")
    }

    fn rx_ring(&self) -> Result<MappedRing, DriverError> {
        self.ring(self.rx_ring_offset, "RX")
    }

    fn wait_for_tx_frame(&self, timeout: Duration) -> Result<bool, DriverError> {
        let Some(tx_event) = &self.tx_event else {
            std::thread::sleep(timeout);
            return Ok(false);
        };

        wait_for_handle(tx_event.as_raw(), timeout)
    }

    fn signal_rx_frame(&self) -> Result<(), DriverError> {
        let Some(rx_event) = &self.rx_event else {
            return Ok(());
        };

        let ok = unsafe { SetEvent(rx_event.as_raw()) };
        if ok == 0 {
            return Err(last_windows_error("SetEvent"));
        }

        Ok(())
    }

    fn ring(&self, offset: u32, name: &'static str) -> Result<MappedRing, DriverError> {
        let offset = offset as usize;
        if offset >= self.mapping_size as usize {
            return Err(DriverError::Abi(format!(
                "{name} ring offset {offset} is outside mapping"
            )));
        }

        unsafe {
            MappedRing::new(
                (self.view as *mut u8).add(offset),
                self.mapping_size as usize - offset,
            )
            .map_err(|e| DriverError::MappedRing(e.to_string()))
        }
    }
}

impl Drop for SharedMemoryMapping {
    fn drop(&mut self) {
        unsafe {
            UnmapViewOfFile(self.view);
        }
    }
}

impl Drop for WindowsDriverBackend {
    fn drop(&mut self) {
        if let Err(e) = self
            .handle
            .ioctl_no_output(ioctl::IOCTL_WHY_USB_SESSION_CLOSE, &self.session_id)
        {
            tracing::warn!(error = %e, "failed to close Windows driver session");
        }
    }
}

impl DriverBackend for WindowsDriverBackend {
    fn attach_device(&self, request: DeviceAttachRequest) -> Result<(), DriverError> {
        let vendor_id = request.vendor_id;
        let product_id = request.product_id;
        let request = ioctl::AttachDeviceRequest::new(
            self.session_id,
            vendor_id,
            product_id,
            request.bus_id,
            request.port_id,
            request.flags,
        );

        self.handle
            .ioctl_no_output(ioctl::IOCTL_WHY_USB_ATTACH_DEVICE, &request)?;
        info!(
            session_id = self.session_id,
            vendor_id = format_args!("{vendor_id:04x}"),
            product_id = format_args!("{product_id:04x}"),
            "sent ATTACH_DEVICE IOCTL"
        );
        Ok(())
    }

    fn detach_device(&self, reason: u32) -> Result<(), DriverError> {
        let request = ioctl::DetachDeviceRequest::new(self.session_id, reason);

        self.handle
            .ioctl_no_output(ioctl::IOCTL_WHY_USB_DETACH_DEVICE, &request)?;
        info!(
            session_id = self.session_id,
            reason, "sent DETACH_DEVICE IOCTL"
        );
        Ok(())
    }

    fn push_rx_bytes(&self, bytes: &[u8]) -> Result<(), DriverError> {
        let Some(shared_memory) = &self.shared_memory else {
            return Err(DriverError::UnsupportedBackend(
                "Windows shared memory is not mapped yet".to_string(),
            ));
        };

        shared_memory
            .rx_ring()?
            .push_frame(bytes)
            .map_err(|e| DriverError::MappedRing(e.to_string()))?;
        shared_memory.signal_rx_frame()
    }

    fn pump_once(&self) -> bool {
        false
    }

    fn poll_tx_frame(&self) -> Result<Option<Frame>, DriverError> {
        let Some(shared_memory) = &self.shared_memory else {
            return Ok(None);
        };

        let Some(bytes) = shared_memory
            .tx_ring()?
            .pop_frame(self.max_frame_size as usize)
            .map_err(|e| DriverError::MappedRing(e.to_string()))?
        else {
            return Ok(None);
        };

        Frame::decode(&bytes)
            .map(Some)
            .map_err(DriverError::MalformedTxFrame)
    }

    fn wait_for_tx_frame(&self, timeout: Duration) -> Result<bool, DriverError> {
        let Some(shared_memory) = &self.shared_memory else {
            std::thread::sleep(timeout);
            return Ok(false);
        };

        shared_memory.wait_for_tx_frame(timeout)
    }
}

struct DeviceHandle(Handle);

unsafe impl Send for DeviceHandle {}
unsafe impl Sync for DeviceHandle {}

impl DeviceHandle {
    fn open(device_path: &str) -> Result<Self, DriverError> {
        let wide_path = wide_null(device_path);
        let handle = unsafe {
            CreateFileW(
                wide_path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null_mut(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                null_mut(),
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            return Err(last_windows_error("CreateFileW"));
        }

        Ok(Self(handle))
    }

    fn as_raw(&self) -> Handle {
        self.0
    }

    fn ioctl<I, O>(&self, code: Dword, input: &I) -> Result<O, DriverError> {
        let mut output = MaybeUninit::<O>::uninit();
        let mut bytes_returned = 0;
        let input_bytes = unsafe { ioctl::as_bytes(input) };
        let ok = unsafe {
            DeviceIoControl(
                self.0,
                code,
                input_bytes.as_ptr() as *mut c_void,
                input_bytes.len() as Dword,
                output.as_mut_ptr() as *mut c_void,
                size_of::<O>() as Dword,
                &mut bytes_returned,
                null_mut(),
            )
        };

        if ok == 0 {
            return Err(last_windows_error("DeviceIoControl"));
        }

        if bytes_returned as usize != size_of::<O>() {
            return Err(DriverError::Abi(format!(
                "IOCTL 0x{code:08x} returned {bytes_returned} bytes, expected {}",
                size_of::<O>()
            )));
        }

        Ok(unsafe { output.assume_init() })
    }

    fn ioctl_no_input<O>(&self, code: Dword) -> Result<O, DriverError> {
        let mut output = MaybeUninit::<O>::uninit();
        let mut bytes_returned = 0;
        let ok = unsafe {
            DeviceIoControl(
                self.0,
                code,
                null_mut(),
                0,
                output.as_mut_ptr() as *mut c_void,
                size_of::<O>() as Dword,
                &mut bytes_returned,
                null_mut(),
            )
        };

        if ok == 0 {
            return Err(last_windows_error("DeviceIoControl"));
        }

        if bytes_returned as usize != size_of::<O>() {
            return Err(DriverError::Abi(format!(
                "IOCTL 0x{code:08x} returned {bytes_returned} bytes, expected {}",
                size_of::<O>()
            )));
        }

        Ok(unsafe { output.assume_init() })
    }

    fn ioctl_no_output<I>(&self, code: Dword, input: &I) -> Result<(), DriverError> {
        let input_bytes = unsafe { ioctl::as_bytes(input) };
        let mut bytes_returned = 0;
        let ok = unsafe {
            DeviceIoControl(
                self.0,
                code,
                input_bytes.as_ptr() as *mut c_void,
                input_bytes.len() as Dword,
                null_mut(),
                0,
                &mut bytes_returned,
                null_mut(),
            )
        };

        if ok == 0 {
            return Err(last_windows_error("DeviceIoControl"));
        }

        Ok(())
    }
}

impl Drop for DeviceHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn validate_header(header: ioctl::AbiHeader, expected_size: usize) -> Result<(), DriverError> {
    header
        .validate(expected_size)
        .map_err(|e| DriverError::Abi(e.to_string()))
}

fn wide_null(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain([0]).collect()
}

fn last_windows_error(operation: &'static str) -> DriverError {
    DriverError::WindowsIo {
        operation,
        source: io::Error::last_os_error(),
    }
}

fn non_null_handle(value: u64) -> Option<Handle> {
    if value == 0 {
        None
    } else {
        Some(value as Handle)
    }
}

fn wait_for_handle(handle: Handle, timeout: Duration) -> Result<bool, DriverError> {
    let timeout_ms = timeout.as_millis().min(Dword::MAX as u128) as Dword;
    let result = unsafe { WaitForSingleObject(handle, timeout_ms) };

    match result {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        WAIT_FAILED => Err(last_windows_error("WaitForSingleObject")),
        other => Err(DriverError::Abi(format!(
            "WaitForSingleObject returned unexpected status 0x{other:08x}"
        ))),
    }
}
