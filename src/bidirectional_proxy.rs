use std::fmt::Debug;
use std::io::{Read, Write};
use std::marker::PhantomData;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Sender;

/**
A transport with an internal lock
*/

pub trait WriteTransport: Send + Sync + 'static + Debug {
    fn write(&mut self, data: &[u8]) -> Result<(), Error>;

    fn flush(&mut self) -> Result<(), Error>;

}
pub trait ReadTransport: Send + 'static + Debug {
    ///Reads as many bytes as possible without blocking.
    fn read_nonblock(&mut self, buf: &mut [u8]) -> Result<usize, Error>;}




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
pub struct BidirectionalProxy {
    data_sender: Sender<Box<[u8]>>,
}

impl BidirectionalProxy {
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

    pub fn send(&self, data: &[u8]) -> Result<(), Error> {
        self.data_sender.send(data.to_vec().into_boxed_slice())
            .map_err(|_| Error::IoError(std::io::Error::new(std::io::ErrorKind::Other, "Failed to send data to proxy")))?;
        Ok(())
    }

}

impl WriteTransport for TcpStream {
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

    fn flush(&mut self) -> Result<(), Error> {
        match std::io::Write::flush(self) {
            Ok(_) => Ok(()),
            Err(e) => Err(Error::IoError(e)),
        }
    }
}

impl ReadTransport for TcpStream {
    fn read_nonblock(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        self.set_nonblocking(true).unwrap();
        match self.read(buf) {
            Ok(size) => Ok(size),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(0), // No data available
            Err(e) => Err(Error::IoError(e)),
        }
    }
}