use bytemuck::{Pod, Zeroable};

/// Defines the exact memory layout of a USB Request Block (URB)
/// that will be sent over the wire without serialization overhead.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct UrbFrame {
    pub urb_id: u64,
    pub endpoint: u8,
    pub direction: u8,
    pub transfer_type: u8,
    pub reserved: u8,
    pub status: i32,
    pub buffer_length: u32,
    pub _pad: u32,
    // Note: In a real implementation, dynamic payload follows this header,
    // or we use a fixed max size. For zero-copy, we typically send the header
    // followed by `buffer_length` bytes of raw payload.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let frame = UrbFrame {
            urb_id: 1,
            endpoint: 0,
            direction: 1, // IN
            transfer_type: 2, // Bulk
            reserved: 0,
            status: 0,
            buffer_length: 512,
            _pad: 0,
        };

        let bytes = bytemuck::bytes_of(&frame);
        assert_eq!(bytes.len(), std::mem::size_of::<UrbFrame>());
    }
}
