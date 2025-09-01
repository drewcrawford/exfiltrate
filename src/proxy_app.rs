//! Proxy application for exfiltrate.
//!
//! This binary provides a standalone proxy server that bridges different transport
//! protocols for the exfiltrate system. It creates a transit proxy that can handle
//! JSON-RPC messages and forward them between different clients and servers.
//!
//! # Overview
//!
//! The proxy application serves as an intermediary for JSON-RPC communication,
//! allowing different components to communicate through a central hub. This is
//! particularly useful for:
//! - Debugging distributed systems
//! - Bridging different transport protocols (HTTP, stdio, WebSocket)
//! - Logging and monitoring RPC traffic
//! - Implementing the Model Context Protocol (MCP) server
//!
//! # Configuration
//!
//! The proxy can be configured to use different transport mechanisms:
//! - **HTTP Server**: Listens on a TCP port (default: 127.0.0.1:1984)
//! - **Stdio Server**: Communicates via standard input/output
//!
//! # Usage
//!
//! Run the proxy with:
//! ```bash
//! cargo run --bin proxy --features transit
//! ```
//!
//! The proxy will start an HTTP server on port 1984 by default. To use stdio
//! mode instead, uncomment the stdio line and comment out the HTTP line.
//!
//! # Examples
//!
//! ## Creating a transit proxy
//!
//! ```
//! # // Mock types for documentation
//! # mod transit {
//! #     pub mod transit_proxy {
//! #         pub struct TransitProxy;
//! #         impl TransitProxy {
//! #             pub fn new() -> Self { TransitProxy }
//! #         }
//! #     }
//! #     pub mod http {
//! #         use super::transit_proxy::TransitProxy;
//! #         pub struct Server;
//! #         impl Server {
//! #             pub fn new(_addr: &str, _proxy: TransitProxy) -> Self { Server }
//! #         }
//! #     }
//! #     pub mod stdio {
//! #         use super::transit_proxy::TransitProxy;
//! #         pub struct Server;
//! #         impl Server {
//! #             pub fn new(_proxy: TransitProxy) -> Self { Server }
//! #         }
//! #     }
//! # }
//! # use transit::transit_proxy::TransitProxy;
//! 
//! // Create a transit proxy for routing messages
//! let transit_proxy = TransitProxy::new();
//! 
//! // The proxy can be used with different server types
//! # // Note: In actual usage, you would keep the server alive
//! ```
//!
//! ## HTTP server configuration
//!
//! ```
//! # mod transit {
//! #     pub mod transit_proxy {
//! #         pub struct TransitProxy;
//! #         impl TransitProxy {
//! #             pub fn new() -> Self { TransitProxy }
//! #         }
//! #     }
//! #     pub mod http {
//! #         use super::transit_proxy::TransitProxy;
//! #         pub struct Server;
//! #         impl Server {
//! #             pub fn new(_addr: &str, _proxy: TransitProxy) -> Self { Server }
//! #         }
//! #     }
//! # }
//! # use transit::transit_proxy::TransitProxy;
//! # use transit::http::Server;
//! 
//! let transit_proxy = TransitProxy::new();
//! 
//! // Configure HTTP server with custom address
//! let server = Server::new("127.0.0.1:8080", transit_proxy);
//! 
//! // Server runs on background threads
//! // In production, you would keep the main thread alive
//! ```
//!
//! ## Stdio server configuration  
//!
//! ```
//! # mod transit {
//! #     pub mod transit_proxy {
//! #         pub struct TransitProxy;
//! #         impl TransitProxy {
//! #             pub fn new() -> Self { TransitProxy }
//! #         }
//! #     }
//! #     pub mod stdio {
//! #         use super::transit_proxy::TransitProxy;
//! #         pub struct Server;
//! #         impl Server {
//! #             pub fn new(_proxy: TransitProxy) -> Self { Server }
//! #         }
//! #     }
//! # }
//! # use transit::transit_proxy::TransitProxy;
//! # use transit::stdio::Server;
//! 
//! let transit_proxy = TransitProxy::new();
//! 
//! // Configure stdio server for IPC communication
//! let server = Server::new(transit_proxy);
//! 
//! // Server communicates via stdin/stdout
//! // In production, you would keep the main thread alive
//! ```
//!
//! # Architecture
//!
//! The proxy uses a multi-threaded architecture without async runtimes:
//! - Each connection is handled in its own thread
//! - Message routing is performed by the TransitProxy
//! - The main thread is parked to keep the application running
//!
//! # Features
//!
//! This binary requires the `transit` feature to be enabled, which includes:
//! - HTTP server support
//! - Stdio communication support  
//! - WebSocket support
//! - Message routing and forwarding

#![cfg(feature="transit")]

use exfiltrate::transit::transit_proxy::TransitProxy;

/// Main entry point for the exfiltrate proxy server.
///
/// Creates a transit proxy and starts an HTTP server listening on port 1984.
/// The server runs on background threads, so the main thread is parked to
/// keep the application alive.
///
/// # Configuration Options
///
/// The function includes commented code for alternative transport modes:
/// - HTTP server mode (default): Listens on 127.0.0.1:1984
/// - Stdio mode: Communicates via standard input/output
///
/// To switch modes, comment/uncomment the appropriate lines.
///
/// # Panics
///
/// The application will panic if:
/// - The specified port is already in use
/// - The server fails to start
/// - Network permissions are insufficient
fn main() {
    let transit_proxy = TransitProxy::new();
    let _proxy = exfiltrate::transit::http::Server::new("127.0.0.1:1984", transit_proxy);
    // let _proxy = exfiltrate::transit::stdio::Server::new(transit_proxy);
    std::thread::park();
}