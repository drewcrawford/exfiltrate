//! Transit proxy system for intercepting and forwarding JSON-RPC communication.
//!
//! The `transit` module provides a proxy infrastructure that sits between a client and
//! a target application, enabling inspection, modification, and augmentation of the
//! communication between them. This is particularly useful for debugging, monitoring,
//! and extending functionality without modifying the target application.
//!
//! # Architecture
//!
//! The transit system consists of several key components:
//!
//! - **Transit Proxy**: The core proxy that manages connections and message routing
//! - **HTTP Server**: Provides HTTP/SSE/WebSocket endpoints for client communication
//! - **Stdio Server**: Alternative server using stdin/stdout for communication
//! - **Log Proxy**: Captures and provides access to logwise logs from the target
//! - **Builtin Tools**: Additional MCP tools available only in the proxy
//!
//! # Usage
//!
//! The transit proxy can be started in two modes:
//!
//! ## HTTP Mode
//! ```
//! # #[cfg(feature = "transit")]
//! # {
//! use exfiltrate::transit::{transit_proxy::TransitProxy, http::Server};
//!
//! // Create a transit proxy
//! let proxy = TransitProxy::new();
//! 
//! // Start HTTP server on port 1984
//! // Note: Server spawns background threads
//! let _server = Server::new("127.0.0.1:1984", proxy);
//! 
//! // Server is now running in the background
//! // In a real application, you would keep the process alive
//! # }
//! ```
//!
//! ## Stdio Mode
//! ```
//! # #[cfg(feature = "transit")]
//! # {
//! use exfiltrate::transit::{transit_proxy::TransitProxy, stdio::Server};
//!
//! // Create a transit proxy
//! let proxy = TransitProxy::new();
//! 
//! // Start stdio server (communicates via stdin/stdout)
//! let _server = Server::new(proxy);
//! 
//! // Server is now processing stdin/stdout in background thread
//! # }
//! ```
//!
//! # Features
//!
//! - **Protocol Support**: HTTP POST, Server-Sent Events (SSE), and WebSocket
//! - **Message Interception**: Inspect and modify JSON-RPC requests and responses
//! - **Tool Injection**: Add proxy-only tools that augment the target's capabilities
//! - **Log Capture**: When used with logwise, captures and provides searchable access to logs
//! - **Fallback Handling**: Provides basic functionality even when target is disconnected
//!
//! # Platform Restrictions
//!
//! This module is not available on wasm32 targets due to threading and networking
//! requirements. Attempting to compile with the `transit` feature on wasm32 will
//! result in a compilation error.

#[cfg(target_arch = "wasm32")]
compile_error!("The `transit` feature is not supported on wasm32 targets. Build for another target or disable the `transit` feature.");

pub mod http;
pub mod transit_proxy;
pub mod stdio;
mod log_proxy;
mod builtin_tools;

