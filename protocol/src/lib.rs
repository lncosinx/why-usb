use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Pod, Zeroable)]
pub struct UrbHeader {
    pub urb_id: u64,
    pub endpoint: u32,
    pub payload_length: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_urb_header_size() {
        assert_eq!(std::mem::size_of::<UrbHeader>(), 16);
    }
}
