use std::fmt::Debug;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

/**
A transport with an internal lock
*/

pub trait Transport: Send + Sync + 'static + Debug {
    fn write_block(&mut self, data: &[u8]) -> Result<(), Error>;

    fn flush(&mut self) -> Result<(), Error>;
    ///Reads as many bytes as possible without blocking.
    fn read_nonblock(&mut self, buf: &mut [u8]) -> Result<usize, Error>;
}




#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

#[derive(Debug)]
struct ReadState {
    buf: Vec<u8>,
}

impl ReadState {
    fn new() -> Self {
        ReadState {
            buf: Vec::new(),
        }
    }

    fn add_bytes(&mut self, bytes: &[u8]) {
        // eprintln!("add_bytes: Adding {} bytes to buffer (current size: {})", bytes.len(), self.buf.len());
        // eprintln!("add_bytes: New bytes: {:?}", &bytes[..bytes.len().min(20)]);
        // eprintln!("add_bytes: Buffer before: {:?}", &self.buf[..self.buf.len().min(20)]);
        self.buf.extend_from_slice(bytes);
        // eprintln!("add_bytes: Buffer after: {:?} (total size: {})", &self.buf[..self.buf.len().min(40)], self.buf.len());
    }

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


#[derive(Debug)]
struct MoveRead<T> {
    transport: Mutex<T>,
    partial_read: Mutex<ReadState>,
}

#[derive(Debug)]
pub struct BidirectionalProxy<T: Transport> {
    move_read: Arc<MoveRead<T>>,
}

impl<T: Transport> BidirectionalProxy<T> {
    pub fn new<F>(transport: T, recv: F) -> Self
    where F: Fn(Box<[u8]>) -> Option<Box<[u8]>> + Send + 'static {
        let read = MoveRead {
            transport: Mutex::new(transport),
            partial_read: Mutex::new(ReadState::new()),
        };

        let read = Arc::new(read);
        let move_read = read.clone();

        crate::sys::thread::Builder::new()
            .name("exfiltrate::BidirectionalProxy".to_owned())
            .spawn(move || {
                loop { //the entire flow
                    let mut buf = vec![0u8; 1024];
                    let mut transport = move_read.transport.lock().unwrap();
                    let mut partial_read = move_read.partial_read.lock().unwrap();
                    let did_read;
                    match transport.read_nonblock(&mut buf) {
                        Ok(size) if size > 0 => {
                            // eprintln!("bidi: Initial read of {} bytes from transport, first 10 bytes: {:?}", size, &buf[..size.min(10)]);
                            partial_read.add_bytes(&buf[0..size]);
                            did_read = true;
                        }
                        Ok(_) => {
                            did_read = false;
                            // eprintln!("No initial data to read from transport, starting read loop");
                        }
                        Err(e) => {
                            eprintln!("Error reading from transport: {}", e);
                            did_read = false;
                            break; // Exit the loop on error
                        }
                    }
                    //now try to pop
                    if let Some(msg) = partial_read.pop_msg() {
                        // eprintln!("Pop message of size {}", msg.len());
                        // Call the provided function with the message
                        let buf = recv(msg);
                        match buf {
                            Some(buf) => {
                                // If the function returns a response, send it back
                                let size = buf.len() as u32;
                                let size_bytes = size.to_le_bytes();
                                // eprintln!("bidi: Sending response of {} bytes, size_bytes: {:?}, first 10 data bytes: {:?}",
                                //           buf.len(), size_bytes, &buf[..buf.len().min(10)]);
                                transport.write_block(&size_bytes).unwrap();
                                transport.write_block(&buf).unwrap();
                                transport.flush().unwrap();
                            }
                            None => {
                                eprintln!("bidi: Function returned None, not sending response");
                                // If the function returns None, do nothing
                            }
                        }
                    }
                    else if !did_read {
                        // eprintln!("bidirectional proxy exiting");
                        //release our locks
                        drop(transport);
                        drop(partial_read);
                        // If no data was read, we can sleep a bit to avoid busy waiting
                        // eprintln!("bidi: No data read, sleeping for 100ms");
                        crate::sys::thread::sleep(crate::sys::time::Duration::from_millis(100));
                    }
                }
                //exit main loop
            }).unwrap();


        BidirectionalProxy { move_read: read }
    }

    pub fn send(&self, data: &[u8]) -> Result<(), Error> {
        //write size
        let size = data.len() as u32;
        eprintln!("bidi: Sending message {:?} to transport {:?}", String::from_utf8_lossy(&data), self.move_read.transport);
        let size_bytes = size.to_le_bytes();
        let mut transport = self.move_read.transport.lock().unwrap();
        transport.write_block(&size_bytes).unwrap();
        transport.write_block(&data).unwrap();
        transport.flush().unwrap();
        Ok(())
    }

}

impl Transport for TcpStream {
    fn write_block(&mut self, data: &[u8]) -> Result<(), Error> {
        self.set_nonblocking(false).unwrap();
        match self.write(data) {
            Ok(size) if size == data.len() => Ok(()),
            Ok(_) => Err(Error::IoError(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "Not all data was written",
            ))),
            Err(e) => Err(Error::IoError(e)),
        }
    }

    fn flush(&mut self) -> Result<(), Error> {
        self.set_nonblocking(false).unwrap();
        match std::io::Write::flush(self) {
            Ok(_) => Ok(()),
            Err(e) => Err(Error::IoError(e)),
        }
    }

    fn read_nonblock(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        self.set_nonblocking(true).unwrap();
        match self.read(buf) {
            Ok(size) => Ok(size),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(0), // No data available
            Err(e) => Err(Error::IoError(e)),
        }
    }
}