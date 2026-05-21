use bytemuck::{Pod, Zeroable};

/// Represents a URB Header exactly 16 bytes for zero-copy transmission.
/// Ensure no implicit struct padding.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct UrbHeader {
    pub seq_num: u32,       // 4 bytes
    pub command: u16,       // 2 bytes
    pub device_id: u16,     // 2 bytes
    pub length: u32,        // 4 bytes
    pub padding: u32,       // 4 bytes (to exactly reach 16 bytes alignment)
}

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }

    #[test]
    fn test_urb_header_size() {
        assert_eq!(mem::size_of::<UrbHeader>(), 16);
    }
}
