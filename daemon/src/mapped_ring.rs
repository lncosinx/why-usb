#![allow(dead_code)]

use crate::ioctl::{WHY_USB_RING_BUFFER_SIZE, WHY_USB_RING_HEADER_SIZE, WHY_USB_RING_MAPPING_SIZE};
use std::fmt;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU64, Ordering};

const FRAME_PREFIX_LEN: usize = 4;

#[derive(Clone, Copy)]
pub struct MappedRing {
    base: NonNull<u8>,
    size: usize,
}

unsafe impl Send for MappedRing {}
unsafe impl Sync for MappedRing {}

impl MappedRing {
    pub unsafe fn new(base: *mut u8, size: usize) -> Result<Self, MappedRingError> {
        let base = NonNull::new(base).ok_or(MappedRingError::NullBase)?;

        if size < WHY_USB_RING_MAPPING_SIZE as usize {
            return Err(MappedRingError::InvalidSize {
                actual: size,
                expected: WHY_USB_RING_MAPPING_SIZE as usize,
            });
        }

        if base.as_ptr() as usize % std::mem::align_of::<AtomicU64>() != 0 {
            return Err(MappedRingError::MisalignedBase);
        }

        Ok(Self { base, size })
    }

    pub fn available_data(&self) -> usize {
        let head = self.head().load(Ordering::Acquire);
        let tail = self.tail().load(Ordering::Acquire);
        (head - tail) as usize
    }

    pub fn available_space(&self) -> usize {
        WHY_USB_RING_BUFFER_SIZE as usize - self.available_data()
    }

    pub fn push_frame(&self, frame: &[u8]) -> Result<(), MappedRingError> {
        let frame_len = u32::try_from(frame.len()).map_err(|_| MappedRingError::FrameTooLarge {
            actual: frame.len(),
            max: u32::MAX as usize,
        })?;
        let total_len = FRAME_PREFIX_LEN + frame.len();

        let tail = self.tail().load(Ordering::Acquire);
        let head = self.head().load(Ordering::Relaxed);
        let available_space = WHY_USB_RING_BUFFER_SIZE as usize - (head - tail) as usize;

        if total_len > available_space {
            return Err(MappedRingError::Full {
                requested: total_len,
                available: available_space,
            });
        }

        self.write_wrapped(head as usize, &frame_len.to_be_bytes());
        self.write_wrapped(head as usize + FRAME_PREFIX_LEN, frame);
        self.head()
            .store(head + total_len as u64, Ordering::Release);
        Ok(())
    }

    pub fn pop_frame(&self, max_len: usize) -> Result<Option<Vec<u8>>, MappedRingError> {
        let head = self.head().load(Ordering::Acquire);
        let tail = self.tail().load(Ordering::Relaxed);
        let available_data = (head - tail) as usize;

        if available_data < FRAME_PREFIX_LEN {
            return Ok(None);
        }

        let mut prefix = [0u8; FRAME_PREFIX_LEN];
        self.read_wrapped(tail as usize, &mut prefix);
        let frame_len = u32::from_be_bytes(prefix) as usize;
        let total_len = FRAME_PREFIX_LEN + frame_len;

        if frame_len > max_len {
            return Err(MappedRingError::FrameTooLarge {
                actual: frame_len,
                max: max_len,
            });
        }

        if available_data < total_len {
            return Ok(None);
        }

        let mut frame = vec![0u8; frame_len];
        self.read_wrapped(tail as usize + FRAME_PREFIX_LEN, &mut frame);
        self.tail()
            .store(tail + total_len as u64, Ordering::Release);
        Ok(Some(frame))
    }

    fn head(&self) -> &AtomicU64 {
        unsafe { &*(self.base.as_ptr() as *const AtomicU64) }
    }

    fn tail(&self) -> &AtomicU64 {
        unsafe { &*(self.base.as_ptr().add(8) as *const AtomicU64) }
    }

    fn data(&self) -> *mut u8 {
        unsafe { self.base.as_ptr().add(WHY_USB_RING_HEADER_SIZE as usize) }
    }

    fn write_wrapped(&self, absolute_offset: usize, src: &[u8]) {
        if src.is_empty() {
            return;
        }

        let offset = absolute_offset % WHY_USB_RING_BUFFER_SIZE as usize;
        let first_len = src.len().min(WHY_USB_RING_BUFFER_SIZE as usize - offset);

        unsafe {
            std::ptr::copy_nonoverlapping(src.as_ptr(), self.data().add(offset), first_len);
            if first_len < src.len() {
                std::ptr::copy_nonoverlapping(
                    src.as_ptr().add(first_len),
                    self.data(),
                    src.len() - first_len,
                );
            }
        }
    }

    fn read_wrapped(&self, absolute_offset: usize, dst: &mut [u8]) {
        if dst.is_empty() {
            return;
        }

        let offset = absolute_offset % WHY_USB_RING_BUFFER_SIZE as usize;
        let first_len = dst.len().min(WHY_USB_RING_BUFFER_SIZE as usize - offset);

        unsafe {
            std::ptr::copy_nonoverlapping(self.data().add(offset), dst.as_mut_ptr(), first_len);
            if first_len < dst.len() {
                std::ptr::copy_nonoverlapping(
                    self.data(),
                    dst.as_mut_ptr().add(first_len),
                    dst.len() - first_len,
                );
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MappedRingError {
    NullBase,
    MisalignedBase,
    InvalidSize { actual: usize, expected: usize },
    Full { requested: usize, available: usize },
    FrameTooLarge { actual: usize, max: usize },
}

impl fmt::Display for MappedRingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NullBase => write!(f, "ring base pointer is null"),
            Self::MisalignedBase => write!(f, "ring base pointer is not 8-byte aligned"),
            Self::InvalidSize { actual, expected } => {
                write!(
                    f,
                    "invalid ring size {actual}, expected at least {expected}"
                )
            }
            Self::Full {
                requested,
                available,
            } => {
                write!(
                    f,
                    "ring is full: requested {requested} bytes, available {available}"
                )
            }
            Self::FrameTooLarge { actual, max } => {
                write!(f, "frame too large: {actual} bytes, max {max}")
            }
        }
    }
}

impl std::error::Error for MappedRingError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn aligned_ring_storage() -> Vec<u64> {
        vec![0; WHY_USB_RING_MAPPING_SIZE as usize / std::mem::size_of::<u64>()]
    }

    #[test]
    fn round_trips_frame() {
        let mut storage = aligned_ring_storage();
        let ring = unsafe {
            MappedRing::new(
                storage.as_mut_ptr() as *mut u8,
                WHY_USB_RING_MAPPING_SIZE as usize,
            )
            .unwrap()
        };

        ring.push_frame(b"hello").unwrap();

        assert_eq!(ring.pop_frame(64).unwrap(), Some(b"hello".to_vec()));
        assert_eq!(ring.pop_frame(64).unwrap(), None);
    }

    #[test]
    fn preserves_frame_boundaries() {
        let mut storage = aligned_ring_storage();
        let ring = unsafe {
            MappedRing::new(
                storage.as_mut_ptr() as *mut u8,
                WHY_USB_RING_MAPPING_SIZE as usize,
            )
            .unwrap()
        };

        ring.push_frame(b"one").unwrap();
        ring.push_frame(b"two-two").unwrap();

        assert_eq!(ring.pop_frame(64).unwrap(), Some(b"one".to_vec()));
        assert_eq!(ring.pop_frame(64).unwrap(), Some(b"two-two".to_vec()));
    }

    #[test]
    fn wraps_at_buffer_end() {
        let mut storage = aligned_ring_storage();
        let ring = unsafe {
            MappedRing::new(
                storage.as_mut_ptr() as *mut u8,
                WHY_USB_RING_MAPPING_SIZE as usize,
            )
            .unwrap()
        };

        let near_end = WHY_USB_RING_BUFFER_SIZE as u64 - 2;
        ring.head().store(near_end, Ordering::Relaxed);
        ring.tail().store(near_end, Ordering::Relaxed);

        ring.push_frame(b"wrap").unwrap();

        assert_eq!(ring.pop_frame(64).unwrap(), Some(b"wrap".to_vec()));
    }
}
