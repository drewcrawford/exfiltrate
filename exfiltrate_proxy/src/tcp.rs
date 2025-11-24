// SPDX-License-Identifier: MIT OR Apache-2.0
use exfiltrate_internal::wire::{ADDR, BACKOFF_DURATION, InFlightMessage};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

/// Starts the TCP server on the configured address (usually 127.0.0.1:1337).
///
/// This server listens for connections from the `exfiltrate` CLI.
/// It manages:
/// *   **Broadcasting**: Messages received from the WebSocket (WASM app) are sent to all connected TCP clients.
/// *   **Forwarding**: Messages received from any TCP client are forwarded to the WebSocket.
pub fn open_tcp(
    to_websocket: std::sync::mpsc::Sender<Vec<u8>>,
    from_websocket: std::sync::mpsc::Receiver<Vec<u8>>,
) {
    let listener = TcpListener::bind(ADDR).unwrap();

    // List of active clients to broadcast messages to
    let clients: Arc<Mutex<Vec<std::sync::mpsc::Sender<Vec<u8>>>>> =
        Arc::new(Mutex::new(Vec::new()));

    // Spawn Distributor thread
    let clients_dist = clients.clone();
    std::thread::spawn(move || {
        loop {
            match from_websocket.recv() {
                Ok(msg) => {
                    eprintln!("Distributor: Received message of size {}", msg.len());
                    let mut clients_guard = clients_dist.lock().unwrap();
                    eprintln!(
                        "Distributor: Broadcasting to {} clients",
                        clients_guard.len()
                    );
                    // Send to all clients, remove dead ones
                    clients_guard.retain(|client| {
                        let r = client.send(msg.clone());
                        if r.is_err() {
                            eprintln!("Distributor: Failed to send to client");
                        }
                        r.is_ok()
                    });
                }
                Err(_) => {
                    eprintln!("WebSocket incoming channel closed");
                    break;
                }
            }
        }
    });

    eprintln!("Listening on {}", ADDR);
    std::thread::Builder::new()
        .name("exfiltrate::listen".to_string())
        .spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        do_stream(stream, to_websocket.clone(), clients.clone());
                    }
                    Err(e) => {
                        panic!("{}", e);
                    }
                }
            }
        })
        .unwrap();
}

fn do_stream(
    mut stream: TcpStream,
    to_websocket: std::sync::mpsc::Sender<Vec<u8>>,
    clients: Arc<Mutex<Vec<std::sync::mpsc::Sender<Vec<u8>>>>>,
) {
    // Register this client for receiving messages
    let (tx, rx) = std::sync::mpsc::channel();
    {
        let mut clients_guard = clients.lock().unwrap();
        clients_guard.push(tx);
    }

    // Spawn Writer Thread (TCP Write)
    let mut write_stream = stream.try_clone().expect("Failed to clone TCP stream");
    std::thread::spawn(move || {
        for msg in rx {
            eprintln!("Writer: Received message of size {}", msg.len());
            // Wrap in frame
            if let Err(e) = exfiltrate_internal::wire::send_socket_frame(&msg, &mut write_stream) {
                eprintln!("TCP write error: {}", e);
                break;
            }
            eprintln!("Writer: Sent message to TCP");
        }
    });

    // Reader Loop (TCP Read)
    std::thread::Builder::new()
        .name("exfiltrate::server do_stream".to_string())
        .spawn(move || {
            let mut in_flight_message = InFlightMessage::new();
            loop {
                let msg = in_flight_message.read_stream(&mut stream);
                match msg {
                    Err(e) => {
                        eprintln!("Error reading inflight message: {:?}", e);
                        return;
                    }
                    Ok(exfiltrate_internal::wire::ReadStatus::WouldBlock) => {
                        std::thread::sleep(BACKOFF_DURATION);
                    }
                    Ok(exfiltrate_internal::wire::ReadStatus::Progress) => {
                        continue;
                    }
                    Ok(exfiltrate_internal::wire::ReadStatus::Completed(pop)) => {
                        // Forward to WebSocket
                        if let Err(e) = to_websocket.send(pop) {
                            eprintln!("Error sending to websocket channel: {}", e);
                            break;
                        }
                    }
                    Ok(_) => {
                        eprintln!("Unknown ReadStatus variant received");
                    }
                }
            }
        })
        .unwrap();
}
