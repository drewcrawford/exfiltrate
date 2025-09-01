//! Bidirectional message-based proxy for communication between components.
//!
//! This module provides a framework for bidirectional, message-based communication
//! using a simple length-prefixed protocol. It abstracts over different transport
//! mechanisms (TCP, WebSocket, etc.) and handles message framing, buffering, and
//! asynchronous processing.
//!
//! # Protocol
//!
//! Messages are transmitted using a simple length-prefixed protocol:
//! - 4 bytes: message length (little-endian u32)
//! - N bytes: message payload
//!
//! This protocol ensures reliable message boundaries even when data arrives
//! fragmented or when multiple messages are received in a single read operation.
//!
//! # Architecture
//!
//! The proxy runs in a dedicated background thread that:
//! 1. Reads incoming messages from the transport using non-blocking I/O
//! 2. Assembles complete messages from potentially fragmented data
//! 3. Processes them through a user-provided callback function
//! 4. Sends optional responses back through the transport
//! 5. Handles outgoing messages queued via the `send` method
//!
//! The architecture is designed to be:
//! - **Non-blocking**: Uses non-blocking I/O to prevent stalls
//! - **Thread-safe**: Can be safely shared across threads
//! - **Transport-agnostic**: Works with any transport implementing the traits
//! - **Efficient**: Minimizes copies and allocations where possible
//!
//!
//! # Thread Safety and Lifetime
//!
//! The proxy spawns a background thread that runs until the transport is closed
//! or an error occurs. The `BidirectionalProxy` struct can be safely cloned and
//! shared across threads to send messages from multiple locations.
//!
//! # Error Handling
//!
//! Errors in the background thread (such as transport failures) will cause the
//! thread to terminate. The proxy will continue to accept `send` calls, but they
//! will fail with a disconnection error.
//!
//! # Platform Compatibility
//!
//! This module uses the `crate::sys::thread` abstraction layer which provides
//! platform-specific threading implementations:
//! - **Native platforms**: Uses OS threads via `std::thread`
//! - **WebAssembly**: Uses Web Workers via `wasm_thread`
//!
//! This ensures the proxy works consistently across all supported platforms.

use std::fmt::Debug;
use std::io::Read;
use std::net::TcpStream;
use std::sync::mpsc::Sender;

/// Trait for transport mechanisms that support writing data.
///
/// This trait abstracts over different transport types (TCP, WebSocket, etc.)
/// that can send data. Implementations must be thread-safe and support
/// non-blocking writes.
///
/// # Implementation Requirements
///
/// - Must be `Send + Sync + 'static` for use across threads
/// - Should handle partial writes internally
/// - Must implement `Debug` for diagnostics
pub trait WriteTransport: Send + Sync + 'static + Debug {
    /// Writes data to the transport.
    ///
    /// Implementations must ensure that either all data is written or an error
    /// is returned. Partial writes should be handled internally or reported
    /// as errors.
    ///
    /// # Arguments
    ///
    /// * `data` - The byte slice to write
    ///
    /// # Returns
    ///
    /// - `Ok(())` if all data was successfully written
    /// - `Err(Error)` if the write failed or was incomplete
    fn write(&mut self, data: &[u8]) -> Result<(), Error>;

    /// Flushes any buffered data to the transport.
    ///
    /// This ensures that all previously written data has been transmitted
    /// to the underlying transport. Implementations should call the underlying
    /// transport's flush mechanism.
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the flush succeeded
    /// - `Err(Error)` if the flush operation failed
    fn flush(&mut self) -> Result<(), Error>;
}
/// Trait for transport mechanisms that support reading data.
///
/// This trait abstracts over different transport types that can receive data.
/// Implementations must support non-blocking reads to avoid stalling the
/// proxy thread.
///
/// # Implementation Requirements
///
/// - Must be `Send + 'static` for use across threads
/// - Must implement `Debug` for diagnostics
/// - Should not block when no data is available
pub trait ReadTransport: Send + 'static + Debug {
    /// Reads as many bytes as possible without blocking.
    ///
    /// This method should attempt to read data into the provided buffer
    /// without blocking. If no data is available, it should return `Ok(0)`
    /// rather than blocking the thread.
    ///
    /// # Arguments
    ///
    /// * `buf` - Buffer to read data into. The size of this buffer determines
    ///           the maximum number of bytes that can be read in one call.
    ///
    /// # Returns
    ///
    /// - `Ok(n)` where `n` is the number of bytes read (0 if no data available)
    /// - `Err(Error)` if a read error occurred (excluding `WouldBlock`)
    ///
    /// # Implementation Notes
    ///
    /// - Must not block if no data is available
    /// - Should convert `WouldBlock` errors to `Ok(0)`
    /// - Other I/O errors should be propagated
    fn read_nonblock(&mut self, buf: &mut [u8]) -> Result<usize, Error>;
}




/// Error type for bidirectional proxy operations.
///
/// This enum encapsulates all possible errors that can occur during
/// proxy operations, primarily wrapping I/O errors from the underlying
/// transport.
///
/// # Examples
///
/// Error handling in practice:
/// ```
/// use std::io;
/// # #[derive(Debug, thiserror::Error)]
/// # pub enum Error {
/// #     #[error("IO error: {0}")]
/// #     IoError(#[from] io::Error),
/// # }
/// 
/// fn handle_error() -> Result<(), Error> {
///     // Errors are typically created from I/O operations
///     Err(Error::IoError(io::Error::new(
///         io::ErrorKind::ConnectionRefused,
///         "Cannot connect to server"
///     )))
/// }
/// 
/// # fn main() {
/// match handle_error() {
///     Err(Error::IoError(e)) if e.kind() == io::ErrorKind::ConnectionRefused => {
///         println!("Connection refused: {}", e);
///     }
///     Err(e) => println!("Other error: {}", e),
///     Ok(_) => println!("Success"),
/// }
/// # }
/// ```
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An I/O error occurred during transport operations.
    /// 
    /// This variant wraps standard I/O errors that may occur during
    /// reading, writing, or flushing data to/from the transport.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Internal state for buffering and parsing incoming messages.
///
/// This struct maintains a buffer of partially received data and provides
/// methods to extract complete messages from the stream. It handles the case
/// where messages arrive fragmented across multiple read operations.
///
/// # Message Assembly
///
/// The `ReadState` accumulates bytes until it has enough data to:
/// 1. Read the 4-byte message length header
/// 2. Extract the complete message body
///
/// Messages may arrive in fragments:
/// - Part of the length header in one read
/// - Rest of the header and part of the body in another read
/// - Multiple complete messages in a single read
///
/// This struct handles all these cases transparently.
#[derive(Debug)]
struct ReadState {
    /// Buffer containing partially received message data.
    /// 
    /// This buffer accumulates bytes from multiple read operations
    /// until complete messages can be extracted.
    buf: Vec<u8>,
}

impl ReadState {
    /// Creates a new empty read state.
    ///
    /// Initializes an empty buffer for accumulating incoming message data.
    /// The buffer will grow as data is added via `add_bytes`.
    ///
    /// # Examples
    ///
    /// ```
    /// # struct ReadState { buf: Vec<u8> }
    /// # impl ReadState {
    /// #     fn new() -> Self { ReadState { buf: Vec::new() } }
    /// # }
    /// let mut state = ReadState::new();
    /// assert!(state.buf.is_empty());
    /// ```
    fn new() -> Self {
        ReadState {
            buf: Vec::new(),
        }
    }

    /// Appends new bytes to the internal buffer.
    ///
    /// This method is called when new data arrives from the transport.
    /// The bytes are appended to the existing buffer, allowing message
    /// assembly across multiple read operations. This is essential for
    /// handling fragmented messages that arrive in multiple chunks.
    ///
    /// # Arguments
    ///
    /// * `bytes` - Slice of bytes to append to the buffer
    ///
    /// # Performance
    ///
    /// The method uses `extend_from_slice` which is optimized for
    /// appending contiguous data to a vector.
    ///
    /// # Examples
    ///
    /// ```
    /// # struct ReadState { buf: Vec<u8> }
    /// # impl ReadState {
    /// #     fn add_bytes(&mut self, bytes: &[u8]) {
    /// #         self.buf.extend_from_slice(bytes);
    /// #     }
    /// # }
    /// # let mut state = ReadState { buf: Vec::new() };
    /// // Simulating fragmented message arrival
    /// state.add_bytes(b"hello");
    /// state.add_bytes(b" world");
    /// assert_eq!(state.buf, b"hello world");
    /// ```
    fn add_bytes(&mut self, bytes: &[u8]) {
        // eprintln!("add_bytes: Adding {} bytes to buffer (current size: {})", bytes.len(), self.buf.len());
        // eprintln!("add_bytes: New bytes: {:?}", &bytes[..bytes.len().min(20)]);
        // eprintln!("add_bytes: Buffer before: {:?}", &self.buf[..self.buf.len().min(20)]);
        self.buf.extend_from_slice(bytes);
        // eprintln!("add_bytes: Buffer after: {:?} (total size: {})", &self.buf[..self.buf.len().min(40)], self.buf.len());
    }

    /// Attempts to extract a complete message from the buffer.
    ///
    /// This method looks for a complete length-prefixed message in the buffer.
    /// If found, it removes the message from the buffer and returns it.
    /// The buffer may contain partial messages, complete messages, or multiple
    /// messages.
    ///
    /// # Message Format
    ///
    /// Messages use a simple length-prefix protocol:
    /// - First 4 bytes: message length (little-endian u32)
    /// - Next N bytes: message payload
    ///
    /// # Returns
    ///
    /// - `Some(message)` if a complete message is available
    /// - `None` if more data is needed to form a complete message
    ///
    /// # Panics
    ///
    /// Panics if a message claims to be larger than 10,000 bytes. This is a
    /// sanity check to prevent memory exhaustion from malformed data.
    ///
    /// # Edge Cases
    ///
    /// - Handles partial length headers (less than 4 bytes)
    /// - Handles partial message bodies
    /// - Preserves remaining data after extracting a message
    /// - Can extract multiple messages in sequence
    ///
    /// # Examples
    ///
    /// ```
    /// # struct ReadState { buf: Vec<u8> }
    /// # impl ReadState {
    /// #     fn pop_msg(&mut self) -> Option<Box<[u8]>> {
    /// #         if self.buf.len() < 4 { return None; }
    /// #         let size = u32::from_le_bytes(self.buf[..4].try_into().unwrap()) as usize;
    /// #         if size > 10_000 { panic!("Message too large"); }
    /// #         if self.buf.len() < size + 4 { return None; }
    /// #         let msg = self.buf[4..size + 4].to_vec().into_boxed_slice();
    /// #         self.buf.drain(..size + 4);
    /// #         Some(msg)
    /// #     }
    /// # }
    /// # let mut state = ReadState { buf: Vec::new() };
    /// // Add a complete message: length=5, data="hello"
    /// let mut msg = vec![];
    /// msg.extend_from_slice(&5u32.to_le_bytes());
    /// msg.extend_from_slice(b"hello");
    /// state.buf = msg;
    /// 
    /// let extracted = state.pop_msg();
    /// assert_eq!(extracted.as_deref(), Some(&b"hello"[..]));
    /// assert!(state.buf.is_empty());
    /// 
    /// // Example with partial message
    /// state.buf = vec![0, 0, 0]; // Only 3 bytes of length header
    /// assert!(state.pop_msg().is_none()); // Not enough data
    /// ```
    fn pop_msg(&mut self) -> Option<Box<[u8]>> {
        // eprintln!("pop_msg: Called with buffer size {}", self.buf.len());
        if self.buf.len() < 4 {
            // eprintln!("pop_msg: Not enough data to read size, current buffer length: {}", self.buf.len());
            return None; // Not enough data to read size
        }

        let size_bytes = &self.buf[..4];
        let size = u32::from_le_bytes(size_bytes.try_into().unwrap()) as usize;
        // eprintln!("pop_msg: Size_bytes: {:?}, size: {:?}, buffer len: {}", size_bytes, size, self.buf.len());
        // eprintln!("pop_msg: Full buffer preview (first 60 bytes): {:?}", &self.buf[..self.buf.len().min(60)]);

        if size > 10_000 {
            eprintln!("ERROR: Invalid message size {} detected. Buffer contents: {:?}", size, &self.buf[..self.buf.len().min(100)]);
            panic!("Probably the wrong size.");
        }

        if self.buf.len() < size + 4 {
            // eprintln!("pop_msg: Not enough data to read full message. Need {}, have {}", size + 4, self.buf.len());
            return None; // Not enough data to read the full message
        }

        // eprintln!("pop_msg: Extracting message from bytes [4..{}]", size + 4);
        let msg = self.buf[4..size + 4].to_vec().into_boxed_slice();
        // eprintln!("pop_msg: Extracted message: {:?}", &msg[..msg.len().min(20)]);
        // eprintln!("pop_msg: About to drain bytes [0..{}] from buffer", size + 4);
        self.buf.drain(..size + 4);
        // eprintln!("pop_msg: Buffer after drain: {:?} (size: {})", &self.buf[..self.buf.len().min(50)], self.buf.len());
        Some(msg)
    }
}



/// A bidirectional message proxy that handles framed message communication.
///
/// `BidirectionalProxy` manages communication between two endpoints using a
/// length-prefixed message protocol. It runs a background thread that:
/// - Reads incoming messages from a transport
/// - Processes them through a user-provided callback
/// - Sends responses back through the transport
/// - Handles outgoing messages queued via the `send` method
///
/// # Thread Safety
///
/// The proxy is thread-safe and can be shared across threads. Messages sent
/// via `send` are queued and transmitted by the background thread.
///

#[derive(Debug)]
pub struct BidirectionalProxy {
    /// Channel sender for queuing outgoing messages.
    data_sender: Sender<Box<[u8]>>,
}

impl BidirectionalProxy {
    /// Creates a new bidirectional proxy with the specified transports and message handler.
    ///
    /// This spawns a background thread that continuously:
    /// 1. Reads data from the read transport using non-blocking I/O
    /// 2. Assembles complete messages from potentially fragmented data
    /// 3. Processes messages through the callback function
    /// 4. Sends responses (if any) back through the write transport
    /// 5. Transmits queued outgoing messages from the send channel
    ///
    /// The background thread runs until the transport encounters an error or
    /// the channel is disconnected.
    ///
    /// # Arguments
    ///
    /// * `write` - Transport for sending data. Must implement `WriteTransport`.
    /// * `read` - Transport for receiving data. Must implement `ReadTransport`.
    /// * `recv` - Callback function to process incoming messages.
    ///            Returns `Some(response)` to send a response, or `None` for no response.
    ///
    /// # Type Parameters
    ///
    /// * `F` - Message handler function type: `Fn(Box<[u8]>) -> Option<Box<[u8]>>`
    /// * `W` - Write transport type implementing `WriteTransport`
    /// * `R` - Read transport type implementing `ReadTransport`
    ///
    /// # Thread Naming
    ///
    /// The background thread is named "exfiltrate::BidirectionalProxy" for
    /// debugging purposes.
    ///
    /// # Example
    ///
   
    pub fn new<F,W,R>(write: W, read: R, recv: F) -> Self
    where F: Fn(Box<[u8]>) -> Option<Box<[u8]>> + Send + 'static,
    R: ReadTransport, W: WriteTransport  {

        let (s, r) = std::sync::mpsc::channel::<Box<[u8]>>();


        crate::sys::thread::Builder::new()
            .name("exfiltrate::BidirectionalProxy".to_owned())
            .spawn(move || {
                let mut read = read;
                let mut write = write;
                // we wind up copying it into here
                let mut partial_read = ReadState::new();
                loop { //the entire flow
                    //todo: this buffer strategy is not as efficient as it could be
                    let mut buf = vec![0; 1024];

                    let mut did_stuff = false;
                    match read.read_nonblock(&mut buf) {
                        Ok(size) if size > 0 => {
                            // eprintln!("bidi: Initial read of {} bytes from transport, first 10 bytes: {:?}", size, &buf[..size.min(10)]);
                            partial_read.add_bytes(&buf[0..size]);
                            did_stuff = true;
                        }
                        Ok(_) => {
                            // eprintln!("No initial data to read from transport, starting read loop");
                        }
                        Err(e) => {
                            eprintln!("Error reading from transport: {}", e);
                            break; // Exit the loop on error
                        }
                    }
                    //now try to pop
                    if let Some(msg) = partial_read.pop_msg() {
                        // eprintln!("Pop message of size {}", msg.len());
                        // Call the provided function with the message
                        did_stuff = true;
                        let buf = recv(msg);
                        match buf {
                            Some(buf) => {
                                // If the function returns a response, send it back
                                let size = buf.len() as u32;
                                let size_bytes = size.to_le_bytes();
                                // eprintln!("bidi: Sending response of {} bytes, size_bytes: {:?}, first 10 data bytes: {:?}",
                                //           buf.len(), size_bytes, &buf[..buf.len().min(10)]);

                                write.write(&size_bytes).unwrap();
                                write.write(&buf).unwrap();
                                write.flush().unwrap();
                            }
                            None => {
                                // eprintln!("bidi: Function returned None, not sending response");
                                // If the function returns None, do nothing
                            }
                        }
                    }
                    //try handling receive queue
                    match r.try_recv() {
                        Ok(msg) => {
                            // eprintln!("bidi: Received message from channel, size: {}", msg.len());
                            let size_bytes = (msg.len() as u32).to_le_bytes();
                            write.write(&size_bytes).unwrap();
                            write.write(&msg).unwrap();
                            write.flush().unwrap();
                            did_stuff = true;
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            // eprintln!("bidi: No messages in channel, continuing");
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            eprintln!("bidi: Channel disconnected, exiting loop");
                            break; // Exit the loop if the channel is disconnected
                        }
                    }
                    if !did_stuff {
                        // eprintln!("bidi: No data processed, sleeping for a bit");
                        std::thread::sleep(std::time::Duration::from_millis(10)); // Sleep to avoid busy waiting
                    }
                }
                //exit main loop
            }).unwrap();


        BidirectionalProxy {  data_sender: s }
    }

    /// Sends a message through the proxy.
    ///
    /// This method queues the message for transmission by the background thread.
    /// The actual transmission happens asynchronously. Messages are sent in the
    /// order they are queued.
    ///
    /// # Arguments
    ///
    /// * `data` - The message data to send as a byte slice
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the message was successfully queued for transmission
    /// - `Err(Error)` if the background thread has terminated
    ///
    /// # Errors
    ///
    /// Returns `Error::IoError` if the background thread has terminated
    /// (channel disconnected), which typically happens when:
    /// - The transport encountered an unrecoverable error
    /// - The connection was closed
    /// - The background thread panicked
    ///
    /// # Message Framing
    ///
    /// The message will be automatically prefixed with its length (4 bytes,
    /// little-endian) before transmission.

    pub fn send(&self, data: &[u8]) -> Result<(), Error> {
        self.data_sender.send(data.to_vec().into_boxed_slice())
            .map_err(|_| Error::IoError(std::io::Error::new(std::io::ErrorKind::Other, "Failed to send data to proxy")))?;
        Ok(())
    }

}

/// Implementation of `WriteTransport` for TCP streams.
///
/// This implementation ensures all data is written to the TCP stream,
/// returning an error if a partial write occurs. The TCP stream must be
/// cloneable (via `try_clone`) to allow separate read and write handles.
impl WriteTransport for TcpStream {
    /// Writes all data to the TCP stream.
    ///
    /// This implementation ensures that all data is written to the stream.
    /// If a partial write occurs (not all bytes are written), an error is returned.
    ///
    /// # Arguments
    ///
    /// * `data` - The complete data to write to the stream
    ///
    /// # Returns
    ///
    /// - `Ok(())` if all data was successfully written
    /// - `Err(Error::IoError)` if the write failed or was partial
    fn write(&mut self, data: &[u8]) -> Result<(), Error> {
        match std::io::Write::write(self,data) {
            Ok(size) if size == data.len() => Ok(()),
            Ok(_) => Err(Error::IoError(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "Not all data was written",
            ))),
            Err(e) => Err(Error::IoError(e)),
        }
    }

    /// Flushes the TCP stream.
    ///
    /// Forces any buffered data to be written to the network immediately.
    /// This is important for ensuring timely delivery of messages.
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the flush succeeded
    /// - `Err(Error::IoError)` if the flush operation failed
    fn flush(&mut self) -> Result<(), Error> {
        match std::io::Write::flush(self) {
            Ok(_) => Ok(()),
            Err(e) => Err(Error::IoError(e)),
        }
    }
}

/// Implementation of `ReadTransport` for TCP streams.
///
/// This implementation sets the stream to non-blocking mode and
/// performs non-blocking reads. The non-blocking behavior prevents
/// the proxy thread from stalling when no data is available.
impl ReadTransport for TcpStream {
    /// Performs a non-blocking read from the TCP stream.
    ///
    /// Sets the stream to non-blocking mode and attempts to read data.
    /// If no data is available (would block), returns 0 rather than blocking
    /// the thread. This allows the proxy to efficiently poll for data without
    /// consuming excessive CPU.
    ///
    /// # Arguments
    ///
    /// * `buf` - Buffer to receive the data
    ///
    /// # Returns
    ///
    /// - `Ok(n)` where `n` is the number of bytes read
    /// - `Ok(0)` if no data is available (would block)
    /// - `Err(Error::IoError)` for other I/O errors
    fn read_nonblock(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        self.set_nonblocking(true).unwrap();
        match self.read(buf) {
            Ok(size) => Ok(size),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(0), // No data available
            Err(e) => Err(Error::IoError(e)),
        }
    }
}