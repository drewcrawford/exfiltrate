//! Internal proxy module for bidirectional JSON-RPC communication.
//!
//! This module provides a cross-platform internal proxy that handles JSON-RPC
//! communication between different parts of the application. It supports both
//! native TCP connections and WebSocket connections for WebAssembly targets.
//!
//! # Architecture
//!
//! The proxy uses a singleton pattern to maintain a single connection throughout
//! the application lifetime. It provides buffering capabilities for notifications
//! that can be sent when the connection becomes available.
//!
//! ```text
//! ┌──────────────┐         ┌─────────────────┐         ┌────────────┐
//! │   Client     │ ──────> │ InternalProxy   │ ──────> │   Server   │
//! │  (logwise,   │  notify │   (singleton)   │  TCP/   │  (1985 or  │
//! │    MCP)      │         │                 │  WS     │   1984)    │
//! └──────────────┘         └─────────────────┘         └────────────┘
//!                               │      ▲
//!                               ▼      │
//!                         ┌──────────────────┐
//!                         │ Buffer Queue     │
//!                         │ (notifications)  │
//!                         └──────────────────┘
//! ```
//!
//! # Platform Support
//!
//! - **Native platforms**: Uses TCP sockets to connect to `127.0.0.1:1985`
//! - **WebAssembly**: Uses WebSocket connections to `ws://localhost:1984`
//!
//! # Internal Usage
//!
//! This module is used internally by other components of the exfiltrate crate,
//! particularly by the logging and MCP (Model Context Protocol) subsystems for
//! handling JSON-RPC communication. The singleton pattern ensures there's only
//! one connection maintained throughout the application lifecycle.
//!
//! # Buffering
//!
//! When the connection is not immediately available, notifications can be buffered
//! and will be automatically sent once the connection is established. This is
//! particularly useful for logging during application startup.
//!
//! # Thread Safety
//!
//! The proxy is designed to be thread-safe. On WebAssembly targets, special care
//! is taken to handle notifications from the main thread without blocking.

mod websocket_adapter;

use crate::bidirectional_proxy::BidirectionalProxy;
use crate::internal_proxy::Error::NotConnected;
use crate::once_nonlock::OnceNonLock;
use std::net::TcpStream;
use std::sync::{Arc, LazyLock, Mutex};

/// Error types for internal proxy operations.
///
/// This enum represents the possible errors that can occur when
/// communicating through the internal proxy.
///
/// # Examples
///
/// ```ignore
/// // ALLOW_IGNORE_DOCTEST: internal_proxy is a private module
/// use crate::internal_proxy::Error;
///
/// // Error can be pattern matched
/// let error = Error::NotConnected;
/// match error {
///     Error::NotConnected => {
///         println!("Connection not available");
///     }
/// }
///
/// // Error implements Debug
/// let error = Error::NotConnected;
/// println!("Error occurred: {:?}", error);
/// ```
#[derive(Debug)]
pub enum Error {
    /// The proxy is not connected to the remote endpoint.
    ///
    /// This error occurs when attempting to send data but no connection
    /// has been established yet.
    NotConnected,
}

/// Global singleton instance of the internal proxy.
///
/// This static instance is lazily initialized on first access and remains
/// alive for the duration of the program.
static INTERNAL_PROXY: LazyLock<InternalProxy> = LazyLock::new(|| InternalProxy::new());

/// Platform-specific write stream type.
///
/// - On native platforms: Uses `TcpStream` for writing
/// - On WebAssembly: Uses `websocket_adapter::WriteAdapter`
#[cfg(not(target_arch = "wasm32"))]
type WriteStream = TcpStream;

/// Platform-specific read stream type.
///
/// - On native platforms: Uses `TcpStream` for reading
/// - On WebAssembly: Uses `websocket_adapter::ReadAdapter`
#[cfg(not(target_arch = "wasm32"))]
type ReadStream = TcpStream;

/// Platform-specific write stream type for WebAssembly.
#[cfg(target_arch = "wasm32")]
type WriteStream = websocket_adapter::WriteAdapter;

/// Platform-specific read stream type for WebAssembly.
#[cfg(target_arch = "wasm32")]
type ReadStream = websocket_adapter::ReadApapter;

/// Internal proxy for handling JSON-RPC communication.
///
/// This struct manages a bidirectional communication channel using either
/// TCP (on native platforms) or WebSocket (on WebAssembly). It provides
/// buffering capabilities for notifications and automatic reconnection.
///
/// # Thread Safety
///
/// The proxy is designed to be thread-safe and uses appropriate synchronization
/// primitives. On WebAssembly, special care is taken since notifications may
/// be sent from the main thread.
///
/// # Buffering
///
/// Notifications can be buffered when the connection is not available.
/// These buffered notifications are automatically sent once the connection
/// is established.
#[derive(Debug)]
pub struct InternalProxy {
    /// Sender for buffering notifications.
    ///
    /// In practice, notifications are sent from the main thread on wasm,
    /// so we can't use a simple Mutex.
    buffered_notification_sender: std::sync::mpsc::Sender<crate::jrpc::Notification>,

    /// Receiver for buffered notifications.
    ///
    /// Protected by a Mutex, but we can simply fail if the lock is contended.
    buffered_notification_receiver: Mutex<std::sync::mpsc::Receiver<crate::jrpc::Notification>>,

    /// The underlying bidirectional proxy for message transport.
    ///
    /// Uses `OnceNonLock` to avoid blocking during initialization.
    bidirectional_proxy: Arc<OnceNonLock<BidirectionalProxy>>,
}

/// Callback function for processing incoming bidirectional messages.
///
/// This function is called by the `BidirectionalProxy` when a message is received.
/// It attempts to parse the message as a JSON-RPC request and dispatch it to
/// the appropriate handler.
///
/// # Arguments
///
/// * `msg` - The raw message bytes received from the remote endpoint
///
/// # Returns
///
/// * `Some(response)` - A serialized JSON-RPC response if the message was a valid request
/// * `None` - If the message could not be processed (currently causes a panic)
///
/// # Panics
///
/// Currently panics if the received message cannot be parsed as a valid JSON-RPC request.
fn bidi_fn(msg: Box<[u8]>) -> Option<Box<[u8]>> {
    //attempt parse as request
    eprintln!(
        "ip: received bidi message: {:?}",
        String::from_utf8_lossy(&msg)
    );
    let request: Result<crate::jrpc::Request, _> = serde_json::from_slice(&msg);
    match request {
        Ok(request) => {
            eprintln!("ip: received request: {:?}", request);
            let response = crate::mcp::dispatch_in_target(request);
            let response_bytes = serde_json::to_vec(&response).unwrap();
            eprintln!(
                "ip: sending response {:?}",
                String::from_utf8_lossy(&response_bytes)
            );
            Some(response_bytes.into_boxed_slice())
        }
        Err(e) => {
            todo!(
                "Not implemented yet: Received request from internal proxy: {:?}",
                e
            );
        }
    }
}

/// The address to connect to for the internal proxy on native platforms.
///
/// This is the TCP address used when running on non-WebAssembly targets.
const ADDR: &str = "127.0.0.1:1985";
impl InternalProxy {
    /// Creates a new instance of the internal proxy.
    ///
    /// This constructor:
    /// 1. Sets up the notification buffering channels
    /// 2. Initializes the bidirectional proxy connection
    /// 3. Attempts an initial connection to the remote endpoint
    ///
    /// The connection attempt is non-blocking and will be retried
    /// automatically when sending notifications.
    fn new() -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        let m = InternalProxy {
            buffered_notification_sender: sender,
            buffered_notification_receiver: Mutex::new(receiver),
            bidirectional_proxy: Arc::new(OnceNonLock::new()),
        };
        m.reconnect_if_possible();
        m
    }

    /// Attempts to establish or re-establish the connection to the remote endpoint.
    ///
    /// This method is platform-specific:
    /// - On native platforms: Attempts a synchronous TCP connection
    /// - On WebAssembly: Initiates an asynchronous WebSocket connection
    ///
    /// The method is non-blocking and will not wait for the connection to complete.
    /// If a connection is already established or in progress, this method does nothing.
    fn reconnect_if_possible(&self) {
        #[cfg(not(target_arch = "wasm32"))]
        self.bidirectional_proxy.try_get_or_init(|| {
            let s = TcpStream::connect(ADDR);
            match s {
                Ok(stream) => {
                    let write_stream = stream
                        .try_clone()
                        .expect("Failed to clone stream for writing");
                    let read_stream = stream;
                    let stream = crate::bidirectional_proxy::BidirectionalProxy::new(
                        write_stream,
                        read_stream,
                        bidi_fn,
                    );
                    Some(stream)
                }
                Err(_e) => return None,
            }
        });
        #[cfg(target_arch = "wasm32")]
        {
            //on wasm, we need to connect asynchronously
            let f = self.bidirectional_proxy.init_async(async move || {
                if web_sys::window().is_none() {
                    crate::internal_proxy::websocket_adapter::patch_close();
                }
                let stream = websocket_adapter::adapter().await;
                match stream {
                    Ok(stream) => {
                        let stream = crate::bidirectional_proxy::BidirectionalProxy::new(
                            stream.0, stream.1, bidi_fn,
                        );
                        Some(stream)
                    }
                    Err(e) => {
                        crate::logging::log(&format!("ip: Failed to connect to {}: {}", ADDR, e));
                        None
                    }
                }
            });
            wasm_bindgen_futures::spawn_local(f)
        }
    }

    /// Sends a JSON-RPC notification through the proxy.
    ///
    /// This method attempts to send a notification immediately. It will first
    /// try to flush any buffered notifications, then send the new notification.
    ///
    /// # Arguments
    ///
    /// * `notification` - The JSON-RPC notification to send
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the notification was successfully sent
    /// * `Err(Error::NotConnected)` - If no connection is available
    ///
    /// # Example
    ///
    /// ```ignore
    /// // ALLOW_IGNORE_DOCTEST: InternalProxy is in a private module not accessible from public API
    /// use exfiltrate::jrpc::Notification;
    /// use exfiltrate::internal_proxy::{InternalProxy, Error};
    /// use serde_json::json;
    ///
    /// let notification = Notification::new(
    ///     "log".to_string(),
    ///     Some(json!({"message": "Hello"}))
    /// );
    ///
    /// let proxy = InternalProxy::current();
    /// match proxy.send_notification(notification) {
    ///     Ok(()) => println!("Notification sent successfully"),
    ///     Err(Error::NotConnected) => {
    ///         println!("Connection not available, notification not sent");
    ///     }
    /// }
    /// ```
    pub fn send_notification(&self, notification: crate::jrpc::Notification) -> Result<(), Error> {
        self.send_buffered_if_possible();
        if let Some(proxy) = self.bidirectional_proxy.get() {
            let msg = serde_json::to_string(&notification).map_err(|_| NotConnected)?;
            proxy.send(msg.as_bytes()).map_err(|_| NotConnected)
        } else {
            //not connected
            Err(NotConnected)
        }
    }
    /// Buffers a notification for later sending.
    ///
    /// This method adds a notification to the buffer and immediately attempts
    /// to send all buffered notifications if a connection is available.
    /// This is useful when you want to ensure notifications are eventually
    /// sent even if the connection is temporarily unavailable.
    ///
    /// # Arguments
    ///
    /// * `notification` - The JSON-RPC notification to buffer
    ///
    /// # Panics
    ///
    /// Panics if the internal channel is disconnected (should not happen in normal operation).
    ///
    /// # Example
    ///
    /// ```ignore
    /// // ALLOW_IGNORE_DOCTEST: InternalProxy is in a private module not accessible from public API
    /// use exfiltrate::jrpc::Notification;
    /// use exfiltrate::internal_proxy::InternalProxy;
    /// use serde_json::json;
    ///
    /// // This is commonly used by the logwise module for buffering log messages
    /// let notification = Notification::new(
    ///     "log".to_string(),
    ///     Some(json!({"level": "info", "message": "Application started"}))
    /// );
    ///
    /// let proxy = InternalProxy::current();
    /// proxy.buffer_notification(notification);
    /// // The notification will be sent when a connection becomes available
    /// ```
    pub fn buffer_notification(&self, notification: crate::jrpc::Notification) {
        self.buffered_notification_sender
            .send(notification)
            .unwrap();
        self.send_buffered_if_possible();
    }

    /// Attempts to send all buffered notifications.
    ///
    /// This method:
    /// 1. Attempts to reconnect if not connected
    /// 2. Tries to acquire the receiver lock (non-blocking)
    /// 3. Drains all buffered notifications
    /// 4. Sends each notification through the proxy
    ///
    /// If the receiver lock is contended, this method will log a message
    /// and return without sending notifications (they remain buffered).
    fn send_buffered_if_possible(&self) {
        self.reconnect_if_possible();
        if let Some(proxy) = self.bidirectional_proxy.get() {
            //short lock
            let mut take = Vec::new();
            if let Some(buffered_receiver) = self.buffered_notification_receiver.try_lock().ok() {
                while let Some(notification) = buffered_receiver.try_recv().ok() {
                    take.push(notification);
                }
            } else {
                crate::logging::log(&"ip: Send contended");
            }
            for notification in take {
                let msg = serde_json::to_string(&notification).unwrap();
                if let Err(e) = proxy.send(msg.as_bytes()) {
                    crate::logging::log(&format!(
                        "ip: Failed to send buffered notification: {}",
                        e
                    ));
                }
            }
        }
    }

    /// Returns the global singleton instance of the internal proxy.
    ///
    /// This method provides access to the single internal proxy instance
    /// that is shared across the entire application. The instance is
    /// lazily initialized on first access.
    ///
    /// # Returns
    ///
    /// A static reference to the global `InternalProxy` instance.
    ///
    /// # Thread Safety
    ///
    /// The returned reference is safe to use from multiple threads concurrently.
    /// All methods on `InternalProxy` are designed to be thread-safe.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // ALLOW_IGNORE_DOCTEST: InternalProxy is in a private module not accessible from public API
    /// use exfiltrate::internal_proxy::InternalProxy;
    /// use exfiltrate::jrpc::Notification;
    /// use serde_json::json;
    ///
    /// // This is the primary way to access the internal proxy
    /// let proxy = InternalProxy::current();
    ///
    /// let notification = Notification::new(
    ///     "status".to_string(),
    ///     Some(json!({"ready": true}))
    /// );
    /// proxy.buffer_notification(notification);
    /// ```
    pub fn current() -> &'static InternalProxy {
        &INTERNAL_PROXY
    }
}
