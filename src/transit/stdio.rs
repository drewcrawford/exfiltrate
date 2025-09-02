// SPDX-License-Identifier: MIT OR Apache-2.0
//! Standard I/O server for the transit proxy.
//!
//! This module provides a server implementation that communicates via
//! stdin/stdout, making it suitable for use in command-line tools and
//! scripts where HTTP/WebSocket connectivity is not needed.

use crate::transit::transit_proxy::TransitProxy;
use std::io::Write;

/// Standard I/O server for the transit proxy system.
///
/// This server reads JSON-RPC messages from stdin and writes responses
/// to stdout, with notifications also sent to stdout. Each message is
/// terminated with a newline character.
///
/// # Example
/// ```no_run
/// # // don't run in doctests
/// # #[cfg(feature = "transit")]
/// # {
/// use exfiltrate::transit::{transit_proxy::TransitProxy, stdio::Server};
///
/// // Create and start a stdio server
/// let proxy = TransitProxy::new();
/// let server = Server::new(proxy);
///
/// // Server now processes stdin/stdout in background thread
/// # }
/// ```
pub struct Server {}

impl Server {
    /// Creates a new stdio server for the transit proxy.
    ///
    /// This spawns a background thread that continuously reads from stdin,
    /// processes JSON-RPC messages through the proxy, and writes responses
    /// to stdout. Notifications from the target are also written to stdout.
    ///
    /// # Arguments
    ///
    /// * `proxy` - The transit proxy that will handle message routing
    ///
    /// # Example
    /// ```no_run
    /// # // don't run in doctests
    /// # #[cfg(feature = "transit")]
    /// # {
    /// use exfiltrate::transit::{transit_proxy::TransitProxy, stdio::Server};
    ///
    /// let proxy = TransitProxy::new();
    /// let server = Server::new(proxy);
    ///
    /// // Server is now running, reading from stdin and writing to stdout
    /// # }
    /// ```
    ///
    /// # Message Format
    ///
    /// - Input: JSON-RPC messages on stdin, one per line
    /// - Output: JSON-RPC responses/notifications on stdout, one per line
    pub fn new(mut proxy: TransitProxy) -> Self {
        proxy.bind(move |msg| {
            let mut stdout = std::io::stdout();
            let bytes = serde_json::to_vec(&msg).unwrap();
            stdout.write_all(&bytes).unwrap();
            stdout.write_all(b"\n").unwrap();
            stdout.flush().unwrap();
        });
        std::thread::Builder::new()
            .name("exfiltrate::stdio".to_string())
            .spawn(move || {
                let stdin = std::io::stdin();
                loop {
                    let mut buffer = String::new();
                    if stdin.read_line(&mut buffer).is_err() {
                        eprintln!("Failed to read from stdin, exiting...");
                        break;
                    }
                    eprintln!("Received data from stdin: {}", buffer);
                    let buffer = buffer.trim().as_bytes();
                    match proxy.received_data(buffer) {
                        Some(response) => {
                            let as_bytes = serde_json::to_vec(&response).unwrap();
                            let mut stdout = std::io::stdout();
                            stdout.write_all(&as_bytes).unwrap();
                            stdout.write_all(b"\n").unwrap();
                            stdout.flush().unwrap();
                        }
                        None => {
                            //nothing?
                        }
                    }
                }
            })
            .unwrap();
        eprintln!("Proxy started on stdin/stdout");
        Server {}
    }
}
