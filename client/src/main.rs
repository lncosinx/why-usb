mod vhci;

use bytes::{BufMut, BytesMut};
use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::time::{sleep, Duration};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use protocol::UrbMessage;
use vhci::VhciAdapter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[Client] Starting why_usb Linux client...");

    // Setup the VHCI adapter (mock)
    let (vhci_adapter, mut vhci_receiver) = VhciAdapter::new();

    // Connect to the Server Daemon on Windows
    let addr = "127.0.0.1:3000";
    println!("[Client] Attempting to connect to Server at {}...", addr);

    // Simple retry loop for connection
    let stream = loop {
        match TcpStream::connect(addr).await {
            Ok(s) => break s,
            Err(e) => {
                eprintln!("[Client] Connection failed: {}. Retrying in 2 seconds...", e);
                sleep(Duration::from_secs(2)).await;
            }
        }
    };

    println!("[Client] Connected to Server!");

    // Disable Nagle's algorithm for low latency
    stream.set_nodelay(true)?;

    // Set up Length-Delimited Codec for fast framing
    let mut framed = Framed::new(stream, LengthDelimitedCodec::new());

    // Spawn a task to simulate local Linux URBs being generated and sent to the Server
    let (tx_sender, mut tx_receiver) = tokio::sync::mpsc::channel::<BytesMut>(100);

    let mock_local_usb_task = tokio::spawn(async move {
        let mut urb_id = 0;
        // Send a mock local URB every 5 seconds
        loop {
            sleep(Duration::from_secs(5)).await;

            urb_id += 1;
            let mut msg = UrbMessage::new(urb_id, 0, 1, 2, 1);
            msg.set_payload(b"Mock URB from Linux Client");

            let mut payload = BytesMut::new();
            payload.put_slice(bytemuck::bytes_of(&msg));

            if tx_sender.send(payload).await.is_err() {
                break;
            }
        }
    });

    // Main loop: Network <-> vhci adapter
    loop {
        tokio::select! {
            // 1. Receive data from Server network, inject into Linux Kernel via VHCI
            result = framed.next() => {
                match result {
                    Some(Ok(bytes)) => {
                        println!("[Client] Received {} bytes from Server. Injecting to VHCI...", bytes.len());
                        if let Err(e) = vhci_adapter.inject_urb(bytes.into()).await {
                            eprintln!("[Client] VHCI inject error: {}", e);
                        }
                    }
                    Some(Err(e)) => {
                        eprintln!("[Client] Network read error: {}", e);
                        break;
                    }
                    None => {
                        println!("[Client] Server disconnected.");
                        break;
                    }
                }
            }

            // 2. Receive data from local Linux Kernel (mocked task), send to Server network
            Some(bytes) = tx_receiver.recv() => {
                println!("[Client] Sending {} bytes to Server...", bytes.len());
                if let Err(e) = framed.send(bytes.into()).await {
                    eprintln!("[Client] Network send error: {}", e);
                    break;
                }
            }

            // 3. Process mock injected URBs (just to clear the channel in our mock setup)
            Some(mock_processed_urb) = vhci_receiver.recv() => {
                println!("[Client] (Mock) VHCI successfully processed URB of size {}", mock_processed_urb.len());
            }
        }
    }

    mock_local_usb_task.abort();
    println!("[Client] Shutdown complete.");
    Ok(())
}
