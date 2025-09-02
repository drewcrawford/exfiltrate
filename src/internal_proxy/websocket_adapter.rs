// SPDX-License-Identifier: MIT OR Apache-2.0
//! WebSocket adapter for WebAssembly targets.
//!
//! This module provides WebSocket-based transport adapters for the internal proxy
//! when running on WebAssembly platforms. It creates a bridge between the WebSocket
//! API available in web browsers and the transport traits used by the bidirectional
//! proxy system.
//!
//! # Architecture
//!
//! The adapter uses a dedicated worker thread to manage WebSocket connections and
//! handle asynchronous WebSocket events. Communication between the main thread and
//! the worker thread is handled through channels.
//!
//! # Components
//!
//! - [`WriteAdapter`]: Implements `WriteTransport` for sending data through WebSocket
//! - [`ReadAdapter`]: Implements `ReadTransport` for receiving data from WebSocket
//! - [`adapter()`]: Main entry point that creates a WebSocket connection and returns
//!   the read/write adapter pair
//!
//! # Thread Model
//!
//! The adapter spawns a single worker thread per process that manages all WebSocket
//! connections. This thread handles:
//! - WebSocket connection establishment
//! - Message routing between WebSocket and the bidirectional proxy
//! - Reconnection logic
//! - Socket lifecycle management

#![cfg(target_arch = "wasm32")]

use super::super::logging::log;
use std::fmt::Display;
use std::sync::{Arc, Mutex};
use wasm_bindgen::JsCast;

use crate::bidirectional_proxy::{ReadTransport, WriteTransport};
use crate::once_nonlock::OnceNonLock;
use wasm_bindgen::closure::Closure;

/// Error types for WebSocket adapter operations.
///
/// # Examples
///
/// ```ignore
/// // ALLOW_IGNORE_DOCTEST: websocket_adapter is in a private module
/// # #[cfg(target_arch = "wasm32")]
/// # {
/// use crate::internal_proxy::websocket_adapter::Error;
///
/// let error = Error::CantConnect("Connection refused".to_string());
/// match error {
///     Error::CantConnect(msg) => {
///         println!("Failed to connect: {}", msg);
///     }
/// }
/// # }
/// ```
#[derive(Debug)]
pub enum Error {
    /// Failed to establish a WebSocket connection.
    ///
    /// Contains a description of the connection failure.
    #[allow(dead_code)]
    CantConnect(String),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            _ => write!(f, "WebsocketAdapter error"),
        }
    }
}

/// A one-shot sender that can only send a value once.
///
/// This is used for sending completion signals from WebSocket
/// event handlers back to the async context. It ensures that
/// only the first event (either success or error) is processed.
struct OneShot<T> {
    c: Arc<Mutex<Option<r#continue::Sender<T>>>>,
}

impl<T> OneShot<T> {
    /// Creates a new one-shot sender.
    fn new(sender: r#continue::Sender<T>) -> Self {
        OneShot {
            c: Arc::new(Mutex::new(Some(sender))),
        }
    }

    /// Sends a value if not already sent.
    ///
    /// This method is idempotent - subsequent calls after the first
    /// successful send will be no-ops.
    fn send_if_needed(&self, value: T) {
        if let Some(sender) = self.c.lock().unwrap().take() {
            sender.send(value);
        }
    }
}

impl<T> Clone for OneShot<T> {
    fn clone(&self) -> Self {
        OneShot {
            c: Arc::clone(&self.c),
        }
    }
}

/// The WebSocket endpoint address.
///
/// This is the address the adapter connects to when establishing
/// a WebSocket connection on WebAssembly platforms.
const ADDR: &str = "ws://localhost:1984";

/// Write adapter for sending data through a WebSocket connection.
///
/// This adapter implements the `WriteTransport` trait, allowing the
/// bidirectional proxy to send data through a WebSocket connection.
/// Data is sent asynchronously through a channel to the worker thread
/// which handles the actual WebSocket transmission.
///
/// # Examples
///
/// ```ignore
/// // ALLOW_IGNORE_DOCTEST: WebAssembly-specific code not testable in regular doctests
/// # #[cfg(target_arch = "wasm32")]
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// use exfiltrate::internal_proxy::websocket_adapter;
/// use exfiltrate::bidirectional_proxy::WriteTransport;
///
/// // Create a WebSocket adapter pair
/// let (mut write_adapter, _read_adapter) = websocket_adapter::adapter().await?;
///
/// // Send data through the WebSocket
/// write_adapter.write(b"Hello, WebSocket!")?;
/// write_adapter.flush()?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct WriteAdapter {
    send: continue_stream::Sender<Vec<u8>>,
}

/// Read adapter for receiving data from a WebSocket connection.
///
/// This adapter implements the `ReadTransport` trait, allowing the
/// bidirectional proxy to receive data from a WebSocket connection.
/// It includes an internal buffer to handle partial reads and ensure
/// efficient data transfer.
///
/// Note: The type name has a typo ("Apapter" instead of "Adapter")
/// but is kept for backward compatibility.
///
/// # Examples
///
/// ```ignore
/// // ALLOW_IGNORE_DOCTEST: WebAssembly-specific code not testable in regular doctests
/// # #[cfg(target_arch = "wasm32")]
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// use exfiltrate::internal_proxy::websocket_adapter;
/// use exfiltrate::bidirectional_proxy::ReadTransport;
///
/// // Create a WebSocket adapter pair
/// let (_write_adapter, mut read_adapter) = websocket_adapter::adapter().await?;
///
/// // Read data from the WebSocket (non-blocking)
/// let mut buffer = [0u8; 1024];
/// match read_adapter.read_nonblock(&mut buffer)? {
///     0 => println!("No data available"),
///     n => println!("Read {} bytes", n),
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct ReadApapter {
    recv: std::sync::mpsc::Receiver<Vec<u8>>,
    buf: Vec<u8>,
}

/// Global channel for sending messages to the WebSocket worker thread.
///
/// This static ensures that only one worker thread is created per process,
/// and provides a way to communicate with that thread.
static SEND_WORKER_MESSAGE: OnceNonLock<continue_stream::Sender<WorkerMessage>> =
    OnceNonLock::new();

/// Message requesting a WebSocket reconnection.
///
/// Contains a channel to send back the result of the connection attempt.
struct ReconnectMessage {
    func_sender: r#continue::Sender<Result<(WriteAdapter, ReadApapter), Error>>,
}

/// Message indicating that a WebSocket has been closed.
struct SocketClosedMessage;

/// Messages that can be sent to the WebSocket worker thread.
enum WorkerMessage {
    /// Request to establish or re-establish a WebSocket connection.
    Reconnect(ReconnectMessage),
    /// Notification that the current WebSocket has been closed.
    SocketClosed(SocketClosedMessage),
}

/// Main worker thread function that manages WebSocket connections.
///
/// This function runs in a dedicated thread and:
/// - Handles connection requests
/// - Manages the WebSocket lifecycle
/// - Routes messages between the WebSocket and the proxy system
///
/// # Arguments
///
/// * `receiver` - Channel for receiving control messages
async fn worker_thread(receiver: continue_stream::Receiver<WorkerMessage>) {
    log("thread started");

    let mut socket = None;

    loop {
        let r = receiver.receive().await;
        match r {
            Some(WorkerMessage::Reconnect(reconnect)) => {
                match &socket {
                    None => {
                        log("WebSocketAdapter: received reconnect message");
                        let (write_send, write_recv) = continue_stream::continuation::<Vec<u8>>();
                        let (read_send, read_recv) = std::sync::mpsc::channel::<Vec<u8>>();

                        let s = create_web_socket(read_send, write_recv).await;
                        match s {
                            Ok(_) => {
                                log("WebSocketAdapter: WebSocket created successfully");
                                socket = Some(s);
                                reconnect.func_sender.send(Ok((
                                    WriteAdapter { send: write_send },
                                    ReadApapter {
                                        recv: read_recv,
                                        buf: Vec::new(),
                                    },
                                )));
                            }
                            Err(e) => {
                                log(&format!(
                                    "WebSocketAdapter: Failed to create WebSocket: {:?}",
                                    e
                                ));
                                reconnect.func_sender.send(Err(e));
                                // Optionally, you could send an error back to the main thread here
                            }
                        }
                    }
                    Some(..) => {
                        //we already have a socket so nothing to do I guess?
                    }
                }
            }
            Some(WorkerMessage::SocketClosed(SocketClosedMessage)) => {
                log("WebSocketAdapter: received socket closed message");
                // Handle socket closed message if needed
                socket = None; // Reset the socket
            }
            None => {
                log("WebSocketAdapter: receiver closed, exiting thread");
                break;
            }
        }
    }
}

/// Creates and configures a WebSocket connection.
///
/// This function:
/// 1. Creates a new WebSocket instance
/// 2. Sets up event handlers for open, error, close, and message events
/// 3. Spawns a task to handle outgoing messages
/// 4. Waits for the connection to be established
///
/// # Arguments
///
/// * `read_send` - Channel for sending received data to the read adapter
/// * `write_recv` - Channel for receiving data to send from the write adapter
///
/// # Returns
///
/// * `Ok(WebSocket)` - If the connection was successfully established
/// * `Err(Error)` - If the connection failed
async fn create_web_socket(
    read_send: std::sync::mpsc::Sender<Vec<u8>>,
    write_recv: continue_stream::Receiver<Vec<u8>>,
) -> Result<web_sys::WebSocket, Error> {
    let ws = web_sys::WebSocket::new(ADDR);
    log("WebSocket created");
    let (func_sender, func_fut) = r#continue::continuation::<Result<(), Error>>();
    let func_sender = OneShot::new(func_sender);
    match ws {
        Ok(ws) => {
            ws.set_binary_type(web_sys::BinaryType::Arraybuffer);
            let move_func_sender = func_sender.clone();
            let onopen_callback = Closure::wrap(Box::new(move |_event: web_sys::Event| {
                web_sys::console::log_1(&"WebSocket opened!".into());
                move_func_sender.send_if_needed(Ok(()));
            }) as Box<dyn FnMut(_)>);
            ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
            onopen_callback.forget(); //leak the closure

            let move_func_sender = func_sender.clone();
            let onerror_callback = Closure::wrap(Box::new(move |event: web_sys::ErrorEvent| {
                // .message seems problematic in some cases?
                let error_description = event.type_();
                let error_msg = format!("Websocket error: {}", error_description);
                web_sys::console::log_1(&error_msg.into());
                move_func_sender.send_if_needed(Err(Error::CantConnect(error_description)));
            }) as Box<dyn FnMut(_)>);
            ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
            onerror_callback.forget(); //leak the closure

            let onclose_callback = Closure::wrap(Box::new(move |_event: web_sys::CloseEvent| {
                web_sys::console::log_1(&"WebSocket closed!".into());
                SEND_WORKER_MESSAGE.get().as_ref().map(|sender| {
                    sender.send(WorkerMessage::SocketClosed(SocketClosedMessage));
                });
            }) as Box<dyn FnMut(_)>);
            ws.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));
            onclose_callback.forget(); //leak the closure
            let onmessage_callback = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
                if let Ok(abuf) = event.data().dyn_into::<web_sys::js_sys::ArrayBuffer>() {
                    let u8_array = web_sys::js_sys::Uint8Array::new(&abuf);
                    let mut vec = vec![0; u8_array.length() as usize];
                    u8_array.copy_to(&mut vec[..]);
                    read_send.send(vec).unwrap();
                } else {
                    let str = format!("Received non-binary message: {:?}", event.data());
                    web_sys::console::log_1(&str.into());
                    unimplemented!("This is not currently supported");
                }
                return;
            }) as Box<dyn FnMut(_)>);
            ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
            onmessage_callback.forget(); //leak the closure

            //set up an async task to read from the stream / send to the websocket
            let move_socket = ws.clone();
            patch_close();
            wasm_bindgen_futures::spawn_local(async move {
                loop {
                    let msg: Option<Vec<u8>> = write_recv.receive().await;
                    // web_sys::console::log_1(&"WebSocketAdapter: will send data".into());
                    if msg.is_none() {
                        web_sys::console::log_1(&"WebSocketAdapter: send_recv closed".into());
                        break;
                    }
                    let msg = msg.unwrap();
                    //can't use send_with_u8_array, see https://github.com/wasm-bindgen/wasm-bindgen/issues/4101
                    let msg = web_sys::js_sys::Uint8Array::from(msg.as_slice());
                    let msg = msg.buffer();
                    let op = move_socket.send_with_array_buffer(&msg);
                    match op {
                        Ok(_) => {
                            // web_sys::console::log_1(&format!("WebSocketAdapter: sent {} bytes", len).into());
                        }
                        Err(e) => {
                            web_sys::console::error_1(
                                &format!("WebSocketAdapter: failed to send data: {:?}", e).into(),
                            );
                            break;
                        }
                    }
                }
            });
            let f = func_fut.await;
            f.map(|_| ws)
        }
        Err(e) => Err(Error::CantConnect(
            e.as_string().unwrap_or_else(|| "Unknown error".to_string()),
        )),
    }
}

/// Creates a WebSocket adapter pair for bidirectional communication.
///
/// This function is the main entry point for establishing a WebSocket connection
/// on WebAssembly platforms. It:
/// 1. Ensures a worker thread is running (creates one if needed)
/// 2. Sends a reconnection request to the worker thread
/// 3. Waits for the connection to be established
/// 4. Returns a pair of adapters for reading and writing
///
/// # Returns
///
/// * `Ok((WriteAdapter, ReadApapter))` - A pair of adapters for bidirectional communication
/// * `Err(Error)` - If the connection could not be established
///
/// # Thread Safety
///
/// This function automatically manages a single worker thread per process.
/// Multiple calls to this function will share the same worker thread but
/// create separate WebSocket connections.
///
/// # Example
///
/// ```ignore
/// // ALLOW_IGNORE_DOCTEST: This is WebAssembly-specific code not testable in regular doctests
/// # #[cfg(target_arch = "wasm32")]
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// use exfiltrate::internal_proxy::websocket_adapter;
/// use exfiltrate::bidirectional_proxy::BidirectionalProxy;
///
/// // Create WebSocket adapters
/// let (write_adapter, read_adapter) = websocket_adapter::adapter().await?;
///
/// // Use with BidirectionalProxy
/// let proxy = BidirectionalProxy::new(
///     write_adapter,
///     read_adapter,
///     |msg| {
///         // Process incoming messages
///         println!("Received {} bytes", msg.len());
///         None // No response
///     }
/// );
/// # Ok(())
/// # }
/// ```
pub async fn adapter() -> Result<(WriteAdapter, ReadApapter), Error> {
    //put ws communication on its own thread
    //one thread only per process!
    SEND_WORKER_MESSAGE.try_get_or_init(move || {
        let (c, r) = continue_stream::continuation();
        crate::sys::thread::Builder::new()
            .name("exfiltrate::WebsocketAdapterWorker".to_owned())
            .spawn(|| {
                patch_close();
                wasm_bindgen_futures::spawn_local(worker_thread(r))
            })
            .expect("Failed to spawn WebsocketAdapter worker thread");
        Some(c)
    });
    match SEND_WORKER_MESSAGE.get().as_ref() {
        Some(sender) => {
            let (func_send, func_recv) =
                r#continue::continuation::<Result<(WriteAdapter, ReadApapter), Error>>();
            //send a reconnect message to the worker thread
            sender.send(WorkerMessage::Reconnect(ReconnectMessage {
                func_sender: func_send,
            }));
            func_recv.await
        }
        None => {
            log("WebsocketAdapter: worker thread not initialized");
            Err(Error::CantConnect(
                "Worker thread not initialized".to_string(),
            ))
        }
    }
}

/// Patches the global `close` function to prevent thread termination.
///
/// On WebAssembly platforms, calling `close()` would terminate the worker thread.
/// This function replaces the global `close` function with a no-op to prevent
/// accidental thread termination, which would break the WebSocket communication.
///
/// This is particularly important for web workers where the default `close()`
/// behavior would terminate the worker context.
///
/// # Safety
///
/// This function modifies global JavaScript behavior and should only be called
/// in WebAssembly worker contexts where thread preservation is critical.
///
/// # Panics
///
/// Panics if the global `close` function cannot be patched.
///
/// # Example
///
/// ```ignore
/// // ALLOW_IGNORE_DOCTEST: WebAssembly-specific code not testable in regular doctests
/// # #[cfg(target_arch = "wasm32")]
/// # {
/// use exfiltrate::internal_proxy::websocket_adapter;
///
/// // Call this in worker threads to prevent accidental termination
/// websocket_adapter::patch_close();
///
/// // Now calling close() will not terminate the thread
/// // (it will just log a message instead)
/// # }
/// ```
pub fn patch_close() {
    //forbid thread exit
    let global = web_sys::js_sys::global();
    let wrapper = Closure::wrap(Box::new(move || {
        web_sys::console::log_1(&"thread close called".into());
    }) as Box<dyn Fn()>);

    web_sys::js_sys::Reflect::set(&global, &"close".into(), wrapper.as_ref().unchecked_ref())
        .expect("failed to patch close");
    wrapper.forget();
}

impl WriteTransport for WriteAdapter {
    /// Writes data to the WebSocket connection.
    ///
    /// The data is sent asynchronously through a channel to the worker thread,
    /// which handles the actual WebSocket transmission.
    ///
    /// # Arguments
    ///
    /// * `data` - The bytes to send through the WebSocket
    ///
    /// # Returns
    ///
    /// Always returns `Ok(())` as sending to the channel is non-blocking.
    fn write(&mut self, data: &[u8]) -> Result<(), crate::bidirectional_proxy::Error> {
        // web_sys::console::log_1(&format!("WebsocketAdapter::write_block: sending {} bytes", data.len()).into());
        self.send.send(data.to_vec());
        Ok(())
    }

    /// Flushes any buffered data.
    ///
    /// For WebSocket connections, this is a no-op as data is sent immediately.
    fn flush(&mut self) -> Result<(), crate::bidirectional_proxy::Error> {
        //nothing to do!
        Ok(())
    }
}
impl ReadTransport for ReadApapter {
    /// Performs a non-blocking read from the WebSocket connection.
    ///
    /// This method:
    /// 1. First checks if there's buffered data from previous reads
    /// 2. If not, attempts to receive new data from the WebSocket
    /// 3. Copies available data to the provided buffer
    /// 4. Stores any excess data in the internal buffer for future reads
    ///
    /// # Arguments
    ///
    /// * `buf` - The buffer to fill with received data
    ///
    /// # Returns
    ///
    /// * `Ok(n)` - The number of bytes read (0 if no data available)
    /// * `Err(_)` - If an error occurred (currently never returns errors)
    fn read_nonblock(
        &mut self,
        buf: &mut [u8],
    ) -> Result<usize, crate::bidirectional_proxy::Error> {
        //copy from self.buf first
        if !self.buf.is_empty() {
            let copy_bytes = std::cmp::min(self.buf.len(), buf.len());
            buf[..copy_bytes].copy_from_slice(&self.buf[..copy_bytes]);
            self.buf.drain(..copy_bytes);
            return Ok(copy_bytes);
        }
        match self.recv.try_recv() {
            Ok(data) => {
                //copy the first part into buf
                let copy_bytes = std::cmp::min(data.len(), buf.len());
                buf[..copy_bytes].copy_from_slice(&data[..copy_bytes]);
                //if there are more bytes, put them in self.buf
                if data.len() > copy_bytes {
                    self.buf.extend_from_slice(&data[copy_bytes..]);
                }
                Ok(copy_bytes)
            }
            Err(_) => Ok(0),
        }
    }
}
