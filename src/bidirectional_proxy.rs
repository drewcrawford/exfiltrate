use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

/**
A transport with an internal lock
*/

pub trait Transport: Send + Sync + 'static {
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
        self.buf.extend_from_slice(bytes);
    }

    fn pop_msg(&mut self) -> Option<Box<[u8]>> {
        if self.buf.len() < 4 {
            eprintln!("Not enough data to read size, current buffer length: {}", self.buf.len());
            return None; // Not enough data to read size
        }

        let size_bytes = &self.buf[..4];
        eprintln!("size_bytes : {:?}", size_bytes);
        let size = u32::from_le_bytes(size_bytes.try_into().unwrap()) as usize;

        if self.buf.len() < size + 4 {
            eprintln!("Not enough data to read {}, current buffer length: {}", size, self.buf.len());
            return None; // Not enough data to read the full message
        }

        let msg = self.buf[4..size + 4].to_vec().into_boxed_slice();
        self.buf.drain(..size + 4); // Remove the processed message from the buffer
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

        std::thread::Builder::new()
            .name("exfiltrate::BidirectionalProxy".to_owned())
            .spawn(move || {
                loop {
                    let mut buf = vec![0u8; 1024];
                    let f = move_read.transport.lock().unwrap().read_nonblock(&mut buf);
                    match f {
                        Ok(size) if size > 0 => {
                            eprintln!("Read {} bytes from transport", size);
                            buf.truncate(size);

                            let mut partial_read = move_read.partial_read.lock().unwrap();
                            partial_read.add_bytes(&buf);
                            drop(partial_read);

                            'next_msg: while let Some(msg) = move_read.partial_read.lock().unwrap().pop_msg() {
                                // Call the provided function with the message
                                let buf = recv(msg);
                                match buf {
                                    Some(buf) => {
                                        // If the function returns a response, send it back
                                        let size = buf.len() as u32;
                                        let mut transport = move_read.transport.lock().unwrap();
                                        let size_bytes = size.to_le_bytes();
                                        transport.write_block(&size_bytes).unwrap();
                                        transport.write_block(&buf).unwrap();
                                        transport.flush().unwrap();
                                    }
                                    None => {
                                        // If the function returns None, do nothing
                                        continue;
                                    }
                                }
                            }
                        }
                        Ok(_) => {
                            // No data read, continue to the next iteration
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                        Err(e) => {
                            eprintln!("Error reading from transport: {}", e);
                            break; // Exit the loop on error
                        }
                    }
                }

            }).unwrap();

        BidirectionalProxy { move_read: read }
    }

    pub fn send(&self, data: &[u8]) -> Result<(), Error> {
        //write size
        let size = data.len() as u32;
        let size_bytes = size.to_le_bytes();
        eprintln!("sending size_bytes : {:?}", size_bytes);
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