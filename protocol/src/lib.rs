use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct UrbHeader {
    pub id: u64,
    pub length: u32,
    pub endpoint: u8,
    pub request_type: u8,
    pub direction: u8, // 0 for out, 1 for in
    pub status: i8,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct UrbMessage {
    pub header: UrbHeader,
    // Provide a small payload for our mock usecase. In a real system,
    // this might use dynamic lengths or flatbuffers, but for a
    // repr(C) bytemuck structure we need a fixed size.
    pub payload: [u8; 64],
}

impl UrbMessage {
    pub fn new(id: u64, length: u32, endpoint: u8, request_type: u8, direction: u8) -> Self {
        UrbMessage {
            header: UrbHeader {
                id,
                length,
                endpoint,
                request_type,
                direction,
                status: 0,
            },
            payload: [0; 64],
        }
    }

    pub fn set_payload(&mut self, data: &[u8]) {
        let len = data.len().min(64);
        self.payload[..len].copy_from_slice(&data[..len]);
        self.header.length = len as u32;
    }
}
