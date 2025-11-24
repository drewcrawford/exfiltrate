use exfiltrate_internal::command::Response;
use exfiltrate_internal::rpc::{CommandResponse, RPC};
use exfiltrate_internal::wire::{BACKOFF_DURATION, InFlightMessage, send_socket_rpc};
use std::net::TcpStream;
use std::sync::{Arc, LazyLock, Mutex};

/// Manages the TCP connection to the remote application.
///
/// Handles sending RPC commands and receiving responses, including
/// reassembling multi-part messages (attachments) and reporting progress
/// for large transfers.
pub struct Client {
    last_reply_id: std::sync::atomic::AtomicU32,
    lock: Arc<Mutex<ClientLock>>,
}

struct ClientLock {
    stream: TcpStream,
    in_flight_message: exfiltrate_internal::wire::InFlightMessage,
}

impl Client {
    fn new() -> Result<Self, String> {
        let stream = match TcpStream::connect(exfiltrate_internal::wire::ADDR) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
                return Err(
                    "The connection was refused; is the debugged application running?".to_string(),
                );
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                return Err(
                    "Permission denied opening a TCP connection; maybe you're in a sandbox?"
                        .to_owned(),
                );
            }
            Err(e) => return Err(e.to_string()),
        };
        let lock = Arc::new(Mutex::new(ClientLock {
            stream,
            in_flight_message: InFlightMessage::new(),
        }));
        let move_lock = lock.clone();
        std::thread::Builder::new()
            .name("wire_client_recv".to_string())
            .spawn(move || {
                let mut in_flight_message = InFlightMessage::new();
                let mut last_print = std::time::Instant::now();
                loop {
                    let msg = in_flight_message.read_stream(&mut move_lock.lock().unwrap().stream);
                    match msg {
                        Err(e) => {
                            eprintln!("Error reading from stream: {}", e);
                            std::process::exit(1);
                        }
                        Ok(exfiltrate_internal::wire::ReadStatus::WouldBlock) => {
                            std::thread::sleep(BACKOFF_DURATION);
                            continue;
                        }
                        Ok(exfiltrate_internal::wire::ReadStatus::Progress) => {
                            //report progress
                            let msg = &mut move_lock.lock().unwrap().in_flight_message;
                            if let Some(expected) = msg.expected_length()
                                && expected > 100_000
                                && last_print.elapsed().as_millis() > 100
                            {
                                let current = msg.current_length();
                                use std::io::Write;
                                eprintln!(
                                    "Received {} / {} bytes ({}%)",
                                    current,
                                    expected,
                                    (current * 100) / (expected as usize)
                                );
                                std::io::stderr().flush().unwrap();
                                last_print = std::time::Instant::now();
                            }
                            continue;
                        }
                        Ok(exfiltrate_internal::wire::ReadStatus::Completed(msg)) => {
                            //parse as RPC
                            let rpc =
                                rmp_serde::from_slice::<RPC>(&msg).expect("Invalid RPC message");
                            match rpc {
                                RPC::Command(_) => {
                                    todo!("Not expecting a command in reply!")
                                }
                                RPC::CommandResponse(response) => {
                                    if response.success {
                                        match response.response {
                                            Response::String(s) => {
                                                eprintln!("{}", s);
                                                std::process::exit(0);
                                            }

                                            _ => {
                                                todo!("Not implemented this response type yet")
                                            }
                                        }
                                    } else {
                                        match response.response {
                                            Response::String(s) => {
                                                eprintln!("Error: {}", s);
                                                std::process::exit(2);
                                            }
                                            _ => {
                                                todo!("Not implemented this response type yet")
                                            }
                                        }
                                    }
                                }
                                _ => {
                                    eprintln!("Unknown RPC variant received");
                                }
                            }
                        }
                        Ok(_) => {
                            eprintln!("Unknown ReadStatus variant received");
                        }
                    }
                }
            })
            .unwrap();
        Ok(Client {
            last_reply_id: 0.into(),
            lock,
        })
    }

    /// Sends an RPC command to the remote application.
    ///
    /// Serializes the RPC message and writes it to the TCP stream.
    pub fn send_rpc(&self, rpc: exfiltrate_internal::rpc::RPC) -> Result<(), std::io::Error> {
        send_socket_rpc(rpc, &mut self.lock.lock().unwrap().stream)?;
        Ok(())
    }
    /// Waits for and retrieves a specific response message.
    ///
    /// Blocks until a response with the matching `reply_id` is received.
    /// Handles:
    /// *   Reading from the stream.
    /// *   Reassembling multi-part attachments.
    /// *   Reporting progress for large transfers to stderr.
    /// *   Filtering out unrelated messages (TODO: currently panics or drops them).
    pub fn pop_msg(&self, reply_id: u32) -> Result<CommandResponse, std::io::Error> {
        let mut lock = self.lock.lock().unwrap();
        let mut last_print = std::time::Instant::now();
        let start_time = std::time::Instant::now();
        let mut waiting_message_printed = false;
        loop {
            //this is needed to destructure two fields
            let lock_ref = &mut *lock;
            let stream = &mut lock_ref.stream;
            let msg = &mut lock_ref.in_flight_message;

            if !waiting_message_printed
                && msg.expected_length().is_none()
                && start_time.elapsed().as_secs() >= 5
            {
                eprintln!("Waiting for reply...");
                waiting_message_printed = true;
            }

            let r = msg.read_stream(stream)?;
            match r {
                exfiltrate_internal::wire::ReadStatus::Completed(message) => {
                    //parse to RPC
                    let rpc_msg: RPC = rmp_serde::from_slice(&message).unwrap();
                    match rpc_msg {
                        RPC::CommandResponse(mut command) => {
                            if command.reply_id == reply_id {
                                if command.num_attachments > 0 {
                                    let mut attachments = Vec::new();
                                    for _ in 0..command.num_attachments {
                                        loop {
                                            // We need to keep reading until we get the attachment
                                            let r = msg.read_stream(stream)?;
                                            match r {
                                                exfiltrate_internal::wire::ReadStatus::Completed(data) => {
                                                    attachments.push(data);
                                                    break;
                                                }
                                                exfiltrate_internal::wire::ReadStatus::WouldBlock => {
                                                    std::thread::sleep(BACKOFF_DURATION);
                                                }
                                                exfiltrate_internal::wire::ReadStatus::Progress => {
                                                     // Reuse the progress reporting logic from the outer loop if possible,
                                                     // or just ignore for now as attachments are parts of the "response"
                                                     // actually, for large files, these attachments ARE the large part.
                                                     // So we should probably report progress.
                                                     // But let's keep it simple for now to ensure correctness.
                                                     // The outer loop's progress reporting relies on `msg.expected_length()`.
                                                     // `read_stream` updates `msg` state.
                                                     // So we can copy the progress logic here.
                                                    if let Some(expected) = msg.expected_length()
                                                        && expected > 100_000
                                                        && last_print.elapsed().as_millis() > 100
                                                    {
                                                        let current = msg.current_length();
                                                        use std::io::Write;
                                                        eprint!(
                                                            "\rReceived attachment part {} / {} bytes ({}%)",
                                                            current,
                                                            expected,
                                                            (current * 100) / (expected as usize)
                                                        );
                                                        std::io::stderr().flush().unwrap();
                                                        last_print = std::time::Instant::now();
                                                    }
                                                }
                                                _ => {
                                                    eprintln!("Unknown ReadStatus variant received");
                                                }
                                            }
                                        }
                                    }
                                    command.response.merge_data(attachments);
                                }
                                return Ok(command);
                            } else {
                                todo!("Need to buffer other messages somewhere")
                            }
                        }
                        _ => {
                            todo!("Other RPC messages not currently handled")
                        }
                    }
                }
                exfiltrate_internal::wire::ReadStatus::Progress => {
                    //report progress
                    let msg = &mut lock_ref.in_flight_message;
                    if let Some(expected) = msg.expected_length()
                        && expected > 100_000
                        && last_print.elapsed().as_millis() > 100
                    {
                        let current = msg.current_length();
                        use std::io::Write;
                        eprint!(
                            "\rReceived {} / {} bytes ({}%)",
                            current,
                            expected,
                            (current * 100) / (expected as usize)
                        );
                        std::io::stderr().flush().unwrap();
                        last_print = std::time::Instant::now();
                    }
                    continue;
                }
                exfiltrate_internal::wire::ReadStatus::WouldBlock => {
                    std::thread::sleep(BACKOFF_DURATION);
                }
                _ => {
                    eprintln!("Unknown ReadStatus variant received");
                }
            }
        }
    }
    /// Generates a unique ID for the next RPC request.
    pub(crate) fn next_reply_id(&self) -> u32 {
        self.last_reply_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }
}

/// Global singleton client instance.
///
/// Lazily initializes the connection to the remote application on first access.
/// Returns an error if the connection fails (e.g., application not running).
pub static CLIENT: LazyLock<Result<Client, String>> = LazyLock::new(Client::new);
