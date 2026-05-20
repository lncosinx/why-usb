use bytes::{BufMut, BytesMut};
use futures::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::time::sleep;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("vhci.h");

        fn init_vhci_driver() -> i32;
        fn cleanup_vhci_driver();

        unsafe fn tx_ring_pop_some(dst: *mut u8, max_len: usize) -> usize;
        unsafe fn rx_ring_push(src: *const u8, len: usize) -> bool;
    }
}

// Constant matching the C++ Ring Buffer size
const MAX_URB_SIZE: usize = 1024 * 64; // Max sensible single read

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[Daemon] Starting why_usb user-mode daemon...");

    // Initialize the Kernel Driver (Mock)
    let status = ffi::init_vhci_driver();
    if status != 0 {
        eprintln!("[Daemon] Failed to initialize VHCI driver, status: {}", status);
        return Err("Driver Init Failed".into());
    }
    println!("[Daemon] Driver initialized successfully.");

    // Start Tokio TCP Server
    let listener = TcpListener::bind("0.0.0.0:3000").await?;
    println!("[Daemon] Listening for Client connections on port 3000...");

    // For simplicity in Phase 3, we accept one connection and bridge it to the rings
    let (stream, addr) = listener.accept().await?;
    println!("[Daemon] Accepted connection from: {}", addr);

    // Disable Nagle's algorithm for low latency as specified
    stream.set_nodelay(true)?;

    // Optimize socket buffer sizes
    let sock = socket2::SockRef::from(&stream);
    let _ = sock.set_recv_buffer_size(MAX_URB_SIZE * 4);
    let _ = sock.set_send_buffer_size(MAX_URB_SIZE * 4);

    // Set up Length-Delimited Codec for fast framing
    let mut framed = Framed::new(stream, LengthDelimitedCodec::new());

    // Create a background task to constantly read from TX_Ring and send to network
    let (tx_sender, mut tx_receiver) = tokio::sync::mpsc::channel::<BytesMut>(100);

    let tx_polling_task = tokio::spawn(async move {
        let mut buffer = vec![0u8; MAX_URB_SIZE];
        loop {
            // Poll the TX Ring from the kernel
            let read_len = unsafe { ffi::tx_ring_pop_some(buffer.as_mut_ptr(), buffer.len()) };

            if read_len > 0 {
                // We got data from the driver! Send it over network
                let mut out_bytes = BytesMut::with_capacity(read_len);
                out_bytes.put_slice(&buffer[..read_len]);

                if tx_sender.send(out_bytes).await.is_err() {
                    break; // Channel closed
                }
            } else {
                // If nothing to read, yield to avoid pegging CPU 100% in our mock
                // In a highly optimized driver, we might use Event signaling
                sleep(Duration::from_micros(100)).await;
            }
        }
    });

    // Main loop: Network <-> RX_Ring / TX_Ring
    loop {
        tokio::select! {
            // 1. Receive data from network, write to RX_Ring (to Kernel)
            result = framed.next() => {
                match result {
                    Some(Ok(bytes)) => {
                        let len = bytes.len();
                        let success = unsafe { ffi::rx_ring_push(bytes.as_ptr(), len) };
                        if !success {
                            eprintln!("[Daemon] RX_Ring overflow! Dropped frame of size {}", len);
                        }
                    }
                    Some(Err(e)) => {
                        eprintln!("[Daemon] Network read error: {}", e);
                        break;
                    }
                    None => {
                        println!("[Daemon] Client disconnected.");
                        break;
                    }
                }
            }

            // 2. Receive data from TX Polling Task, write to network
            Some(bytes) = tx_receiver.recv() => {
                if let Err(e) = framed.send(bytes.into()).await {
                    eprintln!("[Daemon] Network send error: {}", e);
                    break;
                }
            }
        }
    }

    // Cleanup
    tx_polling_task.abort();
    ffi::cleanup_vhci_driver();
    println!("[Daemon] Shutdown complete.");
    Ok(())
}
