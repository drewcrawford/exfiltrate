use crate::rpc::RPC;
use std::io::Write;
use std::mem::MaybeUninit;
use std::net::TcpStream;
use std::time::Duration;

/// The default address for the exfiltrate server.
pub const ADDR: &str = "127.0.0.1:1337";
/// The default backoff duration for retry loops.
pub const BACKOFF_DURATION: Duration = Duration::from_millis(10);

/// The status of a non-blocking read operation.
pub enum ReadStatus {
    /// The read completed with the full message.
    Completed(Vec<u8>),
    /// The read made progress but the message is not complete.
    Progress,
    /// The read would block (no data available).
    WouldBlock,
}

/// Sends an RPC message over a TCP stream.
///
/// The message is serialized to MessagePack and sent as a length-prefixed frame.
pub fn send_socket_rpc(msg: RPC, stream: &mut TcpStream) -> std::io::Result<()> {
    let msgpack_bytes = rmp_serde::to_vec(&msg).unwrap();
    send_socket_frame(&msgpack_bytes, stream)?;
    Ok(())
}

/// Sends a raw byte frame over a TCP stream.
///
/// The frame is prefixed with a 4-byte big-endian length.
pub fn send_socket_frame(msg: &[u8], stream: &mut TcpStream) -> std::io::Result<()> {
    let len: u32 = msg.len().try_into().unwrap();
    // Do not toggle blocking mode to avoid race with reader thread
    // stream.set_nonblocking(false)?;

    write_all_robust(stream, &len.to_be_bytes())?;
    write_all_robust(stream, msg)?;
    Ok(())
}

fn write_all_robust(stream: &mut TcpStream, mut buf: &[u8]) -> std::io::Result<()> {
    while !buf.is_empty() {
        match stream.write(buf) {
            Ok(0) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "failed to write whole buffer",
                ));
            }
            Ok(n) => buf = &buf[n..],
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// A buffer for receiving length-prefixed messages over a stream.
///
/// This struct accumulates bytes from a non-blocking stream until a complete
/// message is available.
pub struct InFlightMessage {
    bytes: Vec<u8>,
    buf: [MaybeUninit<u8>; 1024],
}

impl Default for InFlightMessage {
    fn default() -> Self {
        Self::new()
    }
}

impl InFlightMessage {
    /// Creates a new empty message buffer.
    pub fn new() -> InFlightMessage {
        InFlightMessage {
            bytes: vec![],
            buf: [MaybeUninit::uninit(); 1024],
        }
    }

    /// Appends raw bytes to the message buffer.
    pub fn add_bytes(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    /// Reads from the stream and returns the status of the read operation.
    ///
    /// Returns `ReadStatus::Completed` when a full message is available,
    /// `ReadStatus::Progress` when bytes were read but the message is incomplete,
    /// or `ReadStatus::WouldBlock` when no data is available.
    pub fn read_stream(&mut self, stream: &mut TcpStream) -> std::io::Result<ReadStatus> {
        use std::io::Read;
        // Check if we already have a message buffered
        if let Some(msg) = self.pop_msg() {
            return Ok(ReadStatus::Completed(msg));
        }

        let read_data = unsafe {
            let read_slice =
                std::slice::from_raw_parts_mut(self.buf.as_mut_ptr() as *mut u8, self.buf.len());
            stream.set_nonblocking(true)?;
            let read_size_answer = stream.read(read_slice);
            match read_size_answer {
                Ok(length) => {
                    std::slice::from_raw_parts_mut(self.buf.as_mut_ptr() as *mut u8, length)
                }

                Err(e) => match e.kind() {
                    std::io::ErrorKind::WouldBlock => return Ok(ReadStatus::WouldBlock),
                    _ => {
                        return Err(e);
                    }
                },
            }
        };
        self.add_bytes(read_data);
        if let Some(msg) = self.pop_msg() {
            Ok(ReadStatus::Completed(msg))
        } else if read_data.is_empty() {
            Ok(ReadStatus::WouldBlock)
        } else {
            Ok(ReadStatus::Progress)
        }
    }

    fn pop_msg(&mut self) -> Option<Vec<u8>> {
        if self.bytes.len() < 4 {
            None
        } else {
            let len_bytes: &[u8; 4] = self.bytes[0..4].try_into().unwrap();
            let len = u32::from_be_bytes(*len_bytes);
            let self_bytes_len: u32 = self.bytes.len().try_into().unwrap();
            if self_bytes_len - 4 < len {
                None
            } else {
                //drain length field
                self.bytes.drain(0..4);
                //drain message
                let len_usize: usize = len.try_into().unwrap();
                let msg = self.bytes.drain(0..len_usize);
                Some(msg.collect())
            }
        }
    }

    /// Returns the expected message length from the header, if available.
    pub fn expected_length(&self) -> Option<u32> {
        if self.bytes.len() < 4 {
            None
        } else {
            let len_bytes: &[u8; 4] = self.bytes[0..4].try_into().unwrap();
            let len = u32::from_be_bytes(*len_bytes);
            Some(len)
        }
    }

    /// Returns the number of payload bytes currently buffered (excluding the length header).
    pub fn current_length(&self) -> usize {
        if self.bytes.len() < 4 {
            0
        } else {
            self.bytes.len() - 4
        }
    }
}
