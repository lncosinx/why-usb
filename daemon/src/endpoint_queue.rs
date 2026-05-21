use protocol::{Direction, Frame, FrameType, TransferType};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EndpointKey {
    pub direction: Direction,
    pub transfer_type: TransferType,
    pub endpoint: u8,
}

impl EndpointKey {
    pub fn from_frame(frame: &Frame) -> Self {
        Self {
            direction: frame.direction,
            transfer_type: frame.transfer_type,
            endpoint: frame.endpoint,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedTransfer {
    pub sequence: u64,
    pub key: EndpointKey,
    pub frame: Frame,
}

#[derive(Debug, Default)]
pub struct EndpointTransferQueues {
    next_sequence: u64,
    queues: BTreeMap<EndpointKey, VecDeque<QueuedTransfer>>,
    active_order: VecDeque<EndpointKey>,
    active_keys: BTreeSet<EndpointKey>,
    len: usize,
}

impl EndpointTransferQueues {
    pub fn enqueue(&mut self, frame: Frame) -> Result<u64, EndpointQueueError> {
        if frame.frame_type != FrameType::Request {
            return Err(EndpointQueueError::UnsupportedFrameType(frame.frame_type));
        }

        let key = EndpointKey::from_frame(&frame);
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        let queue = self.queues.entry(key).or_default();
        let was_empty = queue.is_empty();

        queue.push_back(QueuedTransfer {
            sequence,
            key,
            frame,
        });
        self.len += 1;

        if was_empty && self.active_keys.insert(key) {
            self.active_order.push_back(key);
        }

        Ok(sequence)
    }

    pub fn pop_next(&mut self) -> Option<QueuedTransfer> {
        while let Some(key) = self.active_order.pop_front() {
            let Some(queue) = self.queues.get_mut(&key) else {
                self.active_keys.remove(&key);
                continue;
            };
            let Some(transfer) = queue.pop_front() else {
                self.active_keys.remove(&key);
                continue;
            };

            self.len -= 1;
            if queue.is_empty() {
                self.active_keys.remove(&key);
                self.queues.remove(&key);
            } else {
                self.active_order.push_back(key);
            }

            return Some(transfer);
        }

        None
    }

    pub fn cancel(&mut self, request_id: u64) -> Option<QueuedTransfer> {
        let mut empty_key = None;
        let mut removed = None;

        for (key, queue) in self.queues.iter_mut() {
            let Some(index) = queue
                .iter()
                .position(|transfer| transfer.frame.request_id == request_id)
            else {
                continue;
            };

            removed = queue.remove(index);
            if queue.is_empty() {
                empty_key = Some(*key);
            }
            break;
        }

        let transfer = removed?;
        self.len -= 1;
        if let Some(key) = empty_key {
            self.remove_active_key(key);
        }

        Some(transfer)
    }

    pub fn reset_endpoint(&mut self, key: EndpointKey) -> Vec<QueuedTransfer> {
        let Some(queue) = self.queues.remove(&key) else {
            return Vec::new();
        };

        let dropped = queue.len();
        self.len -= dropped;
        self.active_keys.remove(&key);
        self.active_order.retain(|candidate| *candidate != key);
        queue.into_iter().collect()
    }

    pub fn clear(&mut self) -> usize {
        let dropped = self.len;
        self.next_sequence = 0;
        self.queues.clear();
        self.active_order.clear();
        self.active_keys.clear();
        self.len = 0;
        dropped
    }

    pub fn len(&self) -> usize {
        self.len
    }

    fn remove_active_key(&mut self, key: EndpointKey) {
        self.queues.remove(&key);
        self.active_keys.remove(&key);
        self.active_order.retain(|candidate| *candidate != key);
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointQueueError {
    UnsupportedFrameType(FrameType),
}

impl std::fmt::Display for EndpointQueueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedFrameType(frame_type) => {
                write!(f, "unsupported endpoint queue frame type: {frame_type:?}")
            }
        }
    }
}

impl std::error::Error for EndpointQueueError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(request_id: u64, endpoint: u8) -> Frame {
        Frame::new(
            request_id,
            FrameType::Request,
            Direction::HostToDevice,
            TransferType::Bulk,
            endpoint,
            0,
            vec![request_id as u8],
        )
    }

    #[test]
    fn preserves_fifo_order_within_endpoint() {
        let mut queues = EndpointTransferQueues::default();

        queues.enqueue(request(1, 1)).unwrap();
        queues.enqueue(request(2, 1)).unwrap();

        assert_eq!(queues.pop_next().unwrap().frame.request_id, 1);
        assert_eq!(queues.pop_next().unwrap().frame.request_id, 2);
        assert!(queues.is_empty());
    }

    #[test]
    fn round_robins_across_active_endpoints() {
        let mut queues = EndpointTransferQueues::default();

        queues.enqueue(request(1, 1)).unwrap();
        queues.enqueue(request(2, 1)).unwrap();
        queues.enqueue(request(3, 2)).unwrap();

        assert_eq!(queues.pop_next().unwrap().frame.request_id, 1);
        assert_eq!(queues.pop_next().unwrap().frame.request_id, 3);
        assert_eq!(queues.pop_next().unwrap().frame.request_id, 2);
    }

    #[test]
    fn separates_transfer_types_for_same_endpoint_number() {
        let mut queues = EndpointTransferQueues::default();
        let interrupt = Frame::new(
            2,
            FrameType::Request,
            Direction::HostToDevice,
            TransferType::Interrupt,
            1,
            0,
            vec![2],
        );

        queues.enqueue(request(1, 1)).unwrap();
        queues.enqueue(interrupt).unwrap();

        assert_eq!(
            queues.pop_next().unwrap().frame.transfer_type,
            TransferType::Bulk
        );
        assert_eq!(
            queues.pop_next().unwrap().frame.transfer_type,
            TransferType::Interrupt
        );
    }

    #[test]
    fn rejects_non_request_frames() {
        let mut queues = EndpointTransferQueues::default();
        let frame = Frame::detach_request(1);

        assert_eq!(
            queues.enqueue(frame).unwrap_err(),
            EndpointQueueError::UnsupportedFrameType(FrameType::DetachRequest)
        );
    }

    #[test]
    fn clear_drops_pending_transfers_and_resets_state() {
        let mut queues = EndpointTransferQueues::default();

        assert_eq!(queues.enqueue(request(1, 1)).unwrap(), 0);
        assert_eq!(queues.enqueue(request(2, 2)).unwrap(), 1);

        assert_eq!(queues.clear(), 2);
        assert!(queues.is_empty());
        assert_eq!(queues.pop_next(), None);
        assert_eq!(queues.enqueue(request(3, 1)).unwrap(), 0);
        assert_eq!(queues.pop_next().unwrap().frame.request_id, 3);
        assert!(queues.is_empty());
    }

    #[test]
    fn clear_empty_queue_is_noop() {
        let mut queues = EndpointTransferQueues::default();

        assert_eq!(queues.clear(), 0);
        assert!(queues.is_empty());
    }

    #[test]
    fn cancel_removes_pending_transfer_by_request_id() {
        let mut queues = EndpointTransferQueues::default();

        queues.enqueue(request(1, 1)).unwrap();
        queues.enqueue(request(2, 1)).unwrap();
        queues.enqueue(request(3, 2)).unwrap();

        let cancelled = queues.cancel(2).unwrap();

        assert_eq!(cancelled.frame.request_id, 2);
        assert_eq!(queues.len(), 2);
        assert_eq!(queues.pop_next().unwrap().frame.request_id, 1);
        assert_eq!(queues.pop_next().unwrap().frame.request_id, 3);
        assert!(queues.is_empty());
    }

    #[test]
    fn cancel_last_transfer_removes_endpoint_from_active_order() {
        let mut queues = EndpointTransferQueues::default();

        queues.enqueue(request(1, 1)).unwrap();

        assert_eq!(queues.cancel(1).unwrap().frame.request_id, 1);
        assert_eq!(queues.pop_next(), None);
        assert!(queues.is_empty());
    }

    #[test]
    fn reset_endpoint_drops_only_matching_endpoint_queue() {
        let mut queues = EndpointTransferQueues::default();
        let key = EndpointKey {
            direction: Direction::HostToDevice,
            transfer_type: TransferType::Bulk,
            endpoint: 1,
        };

        queues.enqueue(request(1, 1)).unwrap();
        queues.enqueue(request(2, 1)).unwrap();
        queues.enqueue(request(3, 2)).unwrap();

        let dropped = queues.reset_endpoint(key);

        assert_eq!(dropped.len(), 2);
        assert_eq!(dropped[0].frame.request_id, 1);
        assert_eq!(dropped[1].frame.request_id, 2);
        assert_eq!(queues.len(), 1);
        assert_eq!(queues.pop_next().unwrap().frame.request_id, 3);
        assert!(queues.is_empty());
    }
}
