//! The `exfiltrate_proxy` tool bridges the gap between WebAssembly applications and the `exfiltrate` CLI.
//!
//! # Architecture
//!
//! WebAssembly applications running in a browser cannot open raw TCP sockets. They are restricted to WebSockets.
//! The `exfiltrate` CLI, however, communicates via TCP.
//!
//! This proxy solves this by running two servers:
//! 1.  **WebSocket Server (Port 1338)**: Accepts connections from the WASM application.
//! 2.  **TCP Server (Port 1337)**: Accepts connections from the `exfiltrate` CLI.
//!
//! It forwards messages bi-directionally between these two endpoints, allowing the CLI to debug
//! the browser-based application as if it were a local native process.

mod tcp;
mod websocket;

/// Entry point for the proxy.
///
/// Starts both the WebSocket and TCP servers and parks the main thread.
fn main() {
    let (to_ws, from_ws) = websocket::open_websocket();
    tcp::open_tcp(to_ws, from_ws);
    std::thread::park();
}
