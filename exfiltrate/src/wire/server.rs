#[cfg(target_arch = "wasm32")]
mod wasm32;

use crate::commands::COMMANDS;
use exfiltrate_internal::rpc::{CommandInvocation, CommandResponse, RPC};
use exfiltrate_internal::wire::{ADDR, BACKOFF_DURATION, InFlightMessage, send_socket_rpc};
use std::net::{TcpListener, TcpStream};
use std::sync::LazyLock;

/// The exfiltrate server.
///
/// Listens for connections from the CLI and executes commands.
/// The implementation differs based on the target architecture:
/// *   **Native**: Opens a TCP listener on 127.0.0.1:1337.
/// *   **WASM**: Connects to the proxy via WebSocket on 127.0.0.1:1338.
pub struct Server {}

#[cfg(not(target_arch = "wasm32"))]
fn do_stream(mut stream: TcpStream) {
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
                        let rpc = rmp_serde::from_slice::<RPC>(&pop).unwrap();
                        match rpc {
                            RPC::Command(command) => {
                                let response = do_command(command);
                                let reply_id = response.reply_id;
                                //serialize to json
                                let result =
                                    send_socket_rpc(RPC::CommandResponse(response), &mut stream);
                                match result {
                                    Ok(()) => {}
                                    Err(e) => {
                                        eprintln!("Error replying to command {} {}", reply_id, e);
                                    }
                                }
                            }
                            RPC::CommandResponse(_response) => {
                                todo!("Server-side CommandResponse not yet handled")
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
}

fn do_command(command: CommandInvocation) -> CommandResponse {
    for matcher in COMMANDS.lock_sync_read().iter() {
        if matcher.name() == command.name {
            let r = matcher.execute(command.args);
            match r {
                Ok(response) => return CommandResponse::new(true, response, command.reply_id),
                Err(response) => return CommandResponse::new(false, response, command.reply_id),
            }
        }
    }
    let err_msg = format!("command not found: {}", command.name);
    CommandResponse::new(false, err_msg.into(), command.reply_id)
}

/// Global singleton server instance.
///
/// Lazily initializes the server on first access (which happens in `exfiltrate::begin()`).
pub static SERVER: LazyLock<Server> = LazyLock::new(Server::new);

impl Server {
    fn new() -> Server {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self::new_tcp()
        }
        #[cfg(target_arch = "wasm32")]
        {
            Self::new_web()
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn new_tcp() -> Server {
        let listener = match TcpListener::bind(ADDR) {
            Ok(listener) => listener,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                panic!(
                    "Permission denied to open the exfiltrate server socket.  You may be running in a sandbox."
                )
            }
            Err(e) => {
                panic!("Can't open socket: {:?}", e);
            }
        };
        eprintln!("Listening on {}", ADDR);
        std::thread::Builder::new()
            .name("exfiltrate::listen".to_string())
            .spawn(move || {
                for stream in listener.incoming() {
                    match stream {
                        Ok(stream) => {
                            do_stream(stream);
                        }
                        Err(e) => {
                            panic!("{}", e);
                        }
                    }
                }
            })
            .unwrap();
        Server {}
    }

    #[cfg(target_arch = "wasm32")]
    fn new_web() -> Server {
        wasm32::wasm32_go();
        Server {}
    }
}
