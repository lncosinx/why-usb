use bytes::Bytes;
use tokio::sync::mpsc;

// A mock struct for the Linux /dev/vhci adapter
pub struct VhciAdapter {
    // In a real implementation, this would hold a file descriptor to /dev/vhci
    // For the mock, we just use channels to simulate kernel ingestion
    sender: mpsc::Sender<Bytes>,
}

impl VhciAdapter {
    pub fn new() -> (Self, mpsc::Receiver<Bytes>) {
        let (tx, rx) = mpsc::channel(100);
        (VhciAdapter { sender: tx }, rx)
    }

    // Simulate injecting an URB into the Linux USB/IP kernel driver
    pub async fn inject_urb(&self, data: Bytes) -> Result<(), &'static str> {
        // Here we would use `nix::unistd::write` or similar to write to /dev/vhci
        // For the mock, we just send it to our receiver task
        if self.sender.send(data).await.is_err() {
            return Err("Failed to mock inject URB: channel closed");
        }
        Ok(())
    }
}
