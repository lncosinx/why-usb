#![allow(dead_code)]

pub const WHY_USB_ABI_MAGIC: u32 = 0x3159_4857;
pub const WHY_USB_ABI_VERSION: u16 = 1;
pub const WHY_USB_RING_BUFFER_SIZE: u32 = 1024 * 1024;
pub const WHY_USB_RING_HEADER_SIZE: u32 = 16;
pub const WHY_USB_RING_ALIGNMENT: u32 = 8;
pub const WHY_USB_RING_MAPPING_SIZE: u32 = WHY_USB_RING_HEADER_SIZE + WHY_USB_RING_BUFFER_SIZE;
pub const WHY_USB_SHARED_MEMORY_MAGIC: u32 = 0x4d53_5957;
pub const WHY_USB_SHARED_MEMORY_VERSION: u16 = 1;

const FILE_DEVICE_UNKNOWN: u32 = 0x0000_0022;
const METHOD_BUFFERED: u32 = 0;
const FILE_ANY_ACCESS: u32 = 0;

const fn ctl_code(device_type: u32, function: u32, method: u32, access: u32) -> u32 {
    (device_type << 16) | (access << 14) | (function << 2) | method
}

pub const IOCTL_WHY_USB_SESSION_OPEN: u32 =
    ctl_code(FILE_DEVICE_UNKNOWN, 0x801, METHOD_BUFFERED, FILE_ANY_ACCESS);
pub const IOCTL_WHY_USB_SESSION_CLOSE: u32 =
    ctl_code(FILE_DEVICE_UNKNOWN, 0x802, METHOD_BUFFERED, FILE_ANY_ACCESS);
pub const IOCTL_WHY_USB_GET_SHARED_MEMORY: u32 =
    ctl_code(FILE_DEVICE_UNKNOWN, 0x803, METHOD_BUFFERED, FILE_ANY_ACCESS);
pub const IOCTL_WHY_USB_ATTACH_DEVICE: u32 =
    ctl_code(FILE_DEVICE_UNKNOWN, 0x804, METHOD_BUFFERED, FILE_ANY_ACCESS);
pub const IOCTL_WHY_USB_DETACH_DEVICE: u32 =
    ctl_code(FILE_DEVICE_UNKNOWN, 0x805, METHOD_BUFFERED, FILE_ANY_ACCESS);
pub const IOCTL_WHY_USB_GET_STATUS: u32 =
    ctl_code(FILE_DEVICE_UNKNOWN, 0x806, METHOD_BUFFERED, FILE_ANY_ACCESS);

pub unsafe fn as_bytes<T>(value: &T) -> &[u8] {
    unsafe {
        core::slice::from_raw_parts(value as *const T as *const u8, core::mem::size_of::<T>())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum IoctlStatus {
    Ok = 0,
    Unsupported = 1,
    InvalidState = 2,
    BufferTooSmall = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum IoctlSessionState {
    Closed = 0,
    Open = 1,
    Attached = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C, packed)]
pub struct AbiHeader {
    pub magic: u32,
    pub version: u16,
    pub size: u16,
}

impl AbiHeader {
    pub const fn new(size: usize) -> Self {
        Self {
            magic: WHY_USB_ABI_MAGIC,
            version: WHY_USB_ABI_VERSION,
            size: size as u16,
        }
    }

    pub fn validate(self, expected_size: usize) -> Result<(), AbiError> {
        let magic = self.magic;
        let version = self.version;
        let size = self.size;

        if magic != WHY_USB_ABI_MAGIC {
            return Err(AbiError::InvalidMagic(magic));
        }

        if version != WHY_USB_ABI_VERSION {
            return Err(AbiError::UnsupportedVersion(version));
        }

        if size as usize != expected_size {
            return Err(AbiError::InvalidSize {
                actual: size,
                expected: expected_size as u16,
            });
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbiError {
    InvalidMagic(u32),
    UnsupportedVersion(u16),
    InvalidSize { actual: u16, expected: u16 },
    InvalidLayout,
}

impl core::fmt::Display for AbiError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidMagic(magic) => write!(f, "invalid ABI magic 0x{magic:08x}"),
            Self::UnsupportedVersion(version) => write!(f, "unsupported ABI version {version}"),
            Self::InvalidSize { actual, expected } => {
                write!(f, "invalid ABI size {actual}, expected {expected}")
            }
            Self::InvalidLayout => write!(f, "invalid shared memory layout"),
        }
    }
}

impl std::error::Error for AbiError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C, packed)]
pub struct SessionOpenRequest {
    pub header: AbiHeader,
    pub flags: u32,
}

impl SessionOpenRequest {
    pub const fn new(flags: u32) -> Self {
        Self {
            header: AbiHeader::new(core::mem::size_of::<Self>()),
            flags,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C, packed)]
pub struct SessionOpenResponse {
    pub header: AbiHeader,
    pub session_id: u64,
    pub status: u32,
    pub max_frame_size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C, packed)]
pub struct SharedMemoryInfo {
    pub header: AbiHeader,
    pub section_handle: u64,
    pub tx_event_handle: u64,
    pub rx_event_handle: u64,
    pub mapping_size: u32,
    pub tx_ring_size: u32,
    pub rx_ring_size: u32,
    pub tx_ring_offset: u32,
    pub rx_ring_offset: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C, packed)]
pub struct SharedMemoryHeader {
    pub magic: u32,
    pub version: u16,
    pub header_size: u16,
    pub mapping_size: u32,
    pub tx_ring_offset: u32,
    pub rx_ring_offset: u32,
    pub tx_ring_size: u32,
    pub rx_ring_size: u32,
}

impl SharedMemoryHeader {
    pub fn validate(self) -> Result<(), AbiError> {
        let magic = self.magic;
        let version = self.version;
        let header_size = self.header_size;
        let mapping_size = self.mapping_size;
        let tx_ring_offset = self.tx_ring_offset;
        let rx_ring_offset = self.rx_ring_offset;
        let tx_ring_size = self.tx_ring_size;
        let rx_ring_size = self.rx_ring_size;

        if magic != WHY_USB_SHARED_MEMORY_MAGIC {
            return Err(AbiError::InvalidMagic(magic));
        }

        if version != WHY_USB_SHARED_MEMORY_VERSION {
            return Err(AbiError::UnsupportedVersion(version));
        }

        if header_size as usize != core::mem::size_of::<Self>() {
            return Err(AbiError::InvalidSize {
                actual: header_size,
                expected: core::mem::size_of::<Self>() as u16,
            });
        }

        if tx_ring_offset >= mapping_size
            || rx_ring_offset >= mapping_size
            || tx_ring_size == 0
            || rx_ring_size == 0
            || tx_ring_offset % WHY_USB_RING_ALIGNMENT != 0
            || rx_ring_offset % WHY_USB_RING_ALIGNMENT != 0
            || tx_ring_offset.saturating_add(tx_ring_size) > mapping_size
            || rx_ring_offset.saturating_add(rx_ring_size) > mapping_size
        {
            return Err(AbiError::InvalidLayout);
        }

        Ok(())
    }
}

pub const fn align_up(value: u32, alignment: u32) -> u32 {
    (value + (alignment - 1)) & !(alignment - 1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C, packed)]
pub struct AttachDeviceRequest {
    pub header: AbiHeader,
    pub session_id: u64,
    pub vendor_id: u16,
    pub product_id: u16,
    pub bus_id: u8,
    pub port_id: u8,
    pub flags: u16,
}

impl AttachDeviceRequest {
    pub const fn new(
        session_id: u64,
        vendor_id: u16,
        product_id: u16,
        bus_id: u8,
        port_id: u8,
        flags: u16,
    ) -> Self {
        Self {
            header: AbiHeader::new(core::mem::size_of::<Self>()),
            session_id,
            vendor_id,
            product_id,
            bus_id,
            port_id,
            flags,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C, packed)]
pub struct DetachDeviceRequest {
    pub header: AbiHeader,
    pub session_id: u64,
    pub reason: u32,
}

impl DetachDeviceRequest {
    pub const fn new(session_id: u64, reason: u32) -> Self {
        Self {
            header: AbiHeader::new(core::mem::size_of::<Self>()),
            session_id,
            reason,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C, packed)]
pub struct StatusResponse {
    pub header: AbiHeader,
    pub session_id: u64,
    pub status: u32,
    pub session_state: u32,
    pub tx_queued_bytes: u32,
    pub rx_queued_bytes: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, size_of};

    #[test]
    fn ioctl_codes_match_ctl_code_layout() {
        assert_eq!(IOCTL_WHY_USB_SESSION_OPEN, 0x0022_2004);
        assert_eq!(IOCTL_WHY_USB_SESSION_CLOSE, 0x0022_2008);
        assert_eq!(IOCTL_WHY_USB_GET_SHARED_MEMORY, 0x0022_200c);
        assert_eq!(IOCTL_WHY_USB_ATTACH_DEVICE, 0x0022_2010);
        assert_eq!(IOCTL_WHY_USB_DETACH_DEVICE, 0x0022_2014);
        assert_eq!(IOCTL_WHY_USB_GET_STATUS, 0x0022_2018);
    }

    #[test]
    fn abi_structs_are_packed_and_stable() {
        assert_eq!(align_of::<AbiHeader>(), 1);
        assert_eq!(size_of::<AbiHeader>(), 8);
        assert_eq!(size_of::<SessionOpenRequest>(), 12);
        assert_eq!(size_of::<SessionOpenResponse>(), 24);
        assert_eq!(size_of::<SharedMemoryInfo>(), 52);
        assert_eq!(size_of::<SharedMemoryHeader>(), 28);
        assert_eq!(size_of::<AttachDeviceRequest>(), 24);
        assert_eq!(size_of::<DetachDeviceRequest>(), 20);
        assert_eq!(size_of::<StatusResponse>(), 32);
    }

    #[test]
    fn constructors_fill_headers() {
        let open = SessionOpenRequest::new(0);
        assert_eq!(open.header, AbiHeader::new(size_of::<SessionOpenRequest>()));

        let attach = AttachDeviceRequest::new(9, 0x1234, 0x5678, 1, 2, 0);
        assert_eq!(
            attach.header,
            AbiHeader::new(size_of::<AttachDeviceRequest>())
        );
    }

    #[test]
    fn validates_header() {
        let header = AbiHeader::new(size_of::<StatusResponse>());
        header.validate(size_of::<StatusResponse>()).unwrap();

        let bad_version = AbiHeader {
            version: 99,
            ..header
        };

        assert_eq!(
            bad_version
                .validate(size_of::<StatusResponse>())
                .unwrap_err(),
            AbiError::UnsupportedVersion(99)
        );
    }

    #[test]
    fn exposes_struct_as_bytes() {
        let open = SessionOpenRequest::new(7);
        let bytes = unsafe { as_bytes(&open) };

        assert_eq!(bytes.len(), size_of::<SessionOpenRequest>());
        assert_eq!(&bytes[0..4], &WHY_USB_ABI_MAGIC.to_ne_bytes());
    }

    #[test]
    fn validates_shared_memory_header_layout() {
        let header = SharedMemoryHeader {
            magic: WHY_USB_SHARED_MEMORY_MAGIC,
            version: WHY_USB_SHARED_MEMORY_VERSION,
            header_size: size_of::<SharedMemoryHeader>() as u16,
            mapping_size: align_up(
                size_of::<SharedMemoryHeader>() as u32,
                WHY_USB_RING_ALIGNMENT,
            ) + (2 * WHY_USB_RING_MAPPING_SIZE),
            tx_ring_offset: align_up(
                size_of::<SharedMemoryHeader>() as u32,
                WHY_USB_RING_ALIGNMENT,
            ),
            rx_ring_offset: align_up(
                size_of::<SharedMemoryHeader>() as u32,
                WHY_USB_RING_ALIGNMENT,
            ) + WHY_USB_RING_MAPPING_SIZE,
            tx_ring_size: WHY_USB_RING_MAPPING_SIZE,
            rx_ring_size: WHY_USB_RING_MAPPING_SIZE,
        };

        header.validate().unwrap();
    }
}
