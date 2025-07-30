use std::collections::HashMap;
use std::io::{BufRead, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use base64::Engine;
use crate::bidirectional_proxy::{BidirectionalProxy, Error, Transport};
use crate::transit::transit_proxy::{Accept, TransitProxy};


#[derive(PartialEq)]
enum ParseState {
    Method,
    Headers,
    Body(usize),
}

struct HTTPParser {
    buf: Vec<u8>,
}

enum HTTPParseResult {
    NotReady,
    Rejected(String),
    Post(Vec<u8>),
    SSE,
    NotFound,
    Websocket(WebsocketInfo),
}

struct WebsocketInfo {
    key: String,
}

impl HTTPParser {
    fn new() -> Self {
        HTTPParser {
            buf: Vec::new(),
        }
    }

    fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    fn pop(&mut self) -> HTTPParseResult {
        //parse the HTTP header section
        let lines = self.buf.split(|c| *c == b'\n');
        let mut http_lines = Vec::new();
        let mut pos = 0;
        let mut found_blank = false;
        for line in lines {
            if line == b"" {
                //blank line indicates end of headers
                pos += 1; //newline
                found_blank = true;
                break;
            }
            else if line == b"\r" {
                //blank line indicates end of headers
                pos += 2; //carriage return + newline
                found_blank = true;
                break;
            }
            else {
                http_lines.push(line);
                pos += line.len() + 1; //newline
            }

        }
        if !found_blank {
            return HTTPParseResult::NotReady; //not enough data to parse
        }
        let request_line = match http_lines.first() {
            Some(line) => line,
            None => {
                self.buf.clear();
                return HTTPParseResult::Rejected("No request line found".to_string())
            }
        };

        //get method, url and version
        let mut split_line = request_line.split(|&b| b == b' ');
        let method = match split_line.next() {
            Some(method) => method,
            None => {
                let f = format!("Invalid request line: {}", String::from_utf8_lossy(request_line));
                self.buf.clear();
                return HTTPParseResult::Rejected(f);
            },
        };
        let url = match split_line.next() {
            Some(url) => url,
            None => {
                let f = format!("Invalid request line: {}", String::from_utf8_lossy(request_line));
                self.buf.clear();
                return HTTPParseResult::Rejected(f);
            },
        };
        let version = match split_line.next() {
            Some(version) => version,
            None => {
                let f = format!("Invalid request line: {}", String::from_utf8_lossy(request_line));
                self.buf.clear();
                return HTTPParseResult::Rejected(f);
            },
        };
        //the rest of the lines are headers
        let mut headers = HashMap::new();
        for line in &http_lines[1..] {
            let mut split = line.splitn(2, |&b| b == b':');
            let key = match split.next() {
                //http headers are case-insensitive, so we convert to lowercase
                Some(key) => String::from_utf8_lossy(key).trim().to_lowercase().to_owned(),
                None => {
                    let f = format!("Invalid header line: {}", String::from_utf8_lossy(request_line));
                    self.buf.clear();
                    return HTTPParseResult::Rejected(f);
                },
            };
            let val = match split.next() {
                Some(val) => String::from_utf8_lossy(val).trim().to_owned(),
                None => {
                    let f = format!("Invalid header line: {}", String::from_utf8_lossy(request_line));
                    self.buf.clear();
                    return HTTPParseResult::Rejected(f);
                },
            };
            headers.insert(key, val);
        }
        //with that out of the way, let's consider some cases.
        if url != b"/" {
            self.buf.clear();
            return HTTPParseResult::NotFound
        }
        let accept_header = headers.get("accept").map(|s| s.as_str()).unwrap_or("");
        if method == b"GET" && accept_header.contains("text/event-stream") {
            self.buf.clear();
            HTTPParseResult::SSE
        }
        else if method == b"POST" {
            //we need to read the body
            let content_length = match headers.get("content-length") {
                Some(len) => match len.parse::<usize>() {
                    Ok(len) => len,
                    Err(_) => {
                        self.buf.clear();
                        return HTTPParseResult::Rejected(format!("Invalid Content-Length header: {}", len));
                    },
                },
                None =>  {
                    let keys = headers.keys().map(|k| k.to_string()).collect::<Vec<_>>();
                    let msg = format!("Content-Length header not found. Headers: {:?}", keys);
                    self.buf.clear();
                    return HTTPParseResult::Rejected(msg);
                }
            };
            if self.buf.len() < pos + content_length {
                return HTTPParseResult::NotReady; //not enough data to parse
            }
            let body = self.buf[pos..pos + content_length].to_vec();
            self.buf.clear();
            HTTPParseResult::Post(body)
        } else if method == b"GET" && headers.get("upgrade").map(|s| s.as_str()) == Some("websocket") {
            let key = headers.get("sec-websocket-key").map(|s| s.as_str()).unwrap_or("").to_owned();
            HTTPParseResult::Websocket(WebsocketInfo {key})
        }
        else {
            let f = format!("request {}", String::from_utf8_lossy(&self.buf));
            self.buf.clear();
            HTTPParseResult::Rejected(f)
        }



    }
}

#[derive(Debug)]
struct WebsocketStream {
    tcp: TcpStream,
    in_buf: Vec<u8>,
    out_buf: Vec<u8>,
}

impl WebsocketStream {
    fn new(tcp: TcpStream) -> Self {
        WebsocketStream {
            tcp,
            in_buf: Vec::new(),
            out_buf: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub enum WebSocketOrStream {
    WebSocket(WebsocketStream),
    Stream(TcpStream),
}
impl Transport for WebSocketOrStream {
    fn write_block(&mut self, data: &[u8]) -> Result<(), Error> {
        match self {
            WebSocketOrStream::Stream(stream) => {
                stream.write_block(data)?;
                Ok(())
            }
            WebSocketOrStream::WebSocket(stream) => {
                let frame = WebsocketFrame::new(data.to_vec(), false);
                let bytes = frame.to_bytes();
                stream.tcp.write_block(&bytes)?;
                Ok(())
            }
        }
    }

    fn flush(&mut self) -> Result<(), Error> {
        match self {
            WebSocketOrStream::Stream(stream) => {
                Transport::flush(stream)?;
                Ok(())
            }
            WebSocketOrStream::WebSocket(stream) => {
                Transport::flush(&mut stream.tcp)?;
                Ok(())
            }
        }
    }

    fn read_nonblock(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        match self {
            WebSocketOrStream::Stream(stream) => {
                let bytes_read = stream.read_nonblock(buf)?;
                Ok(bytes_read)
            }
            WebSocketOrStream::WebSocket(stream) => {
                if !stream.out_buf.is_empty() {
                    //just return the data from the output buffer
                    let bytes_to_copy = stream.out_buf.len().min(buf.len());
                    buf[..bytes_to_copy].copy_from_slice(&stream.out_buf[..bytes_to_copy]);
                    stream.out_buf.clear();
                    return Ok(bytes_to_copy);
                }
                //otherwise do a read
                let mut private_buf = vec![0; 1024]; //temporary buffer
                //read until we see 0
                loop {
                    let bytes_read = stream.tcp.read_nonblock(&mut private_buf)?;
                    if bytes_read == 0 { break }
                    stream.in_buf.extend_from_slice(&private_buf[..bytes_read]);
                }
                //try to parse a frame
                match WebsocketFrame::from_bytes(&stream.in_buf) {
                    Ok((mut frame, size)) => {
                        eprintln!("WebSocket Frame Parsed with size {}",size);
                        //copy the data to the output buffer
                        let bytes_to_copy = frame.data.len().min(buf.len());
                        buf[..bytes_to_copy].copy_from_slice(&frame.data[..bytes_to_copy]);
                        //remove the bytes from the input buffer
                        stream.in_buf.drain(..size);
                        //place additional bytes in the output buffer
                        if frame.data.len() > bytes_to_copy {
                            stream.out_buf.extend_from_slice(&frame.data[bytes_to_copy..]);
                        }
                        Ok(bytes_to_copy)
                    }
                    Err(WebsocketFrameError::FrameTooShort) => {
                        Ok(0) //not enough data to parse a frame
                    }
                    Err(WebsocketFrameError::Rejected(reason)) => {
                        eprintln!("WebSocket Frame Rejected: {}", reason);
                        stream.in_buf.drain(..);
                        Err(Error::IoError(std::io::Error::new(std::io::ErrorKind::InvalidData, reason)))
                    }

                }

            }
        }
    }
}


pub struct Server {
}

pub struct MessageQueue {
    stream: TcpStream,
}

impl MessageQueue {
    fn new(stream: TcpStream) -> Self {
        Self {
            stream,
        }
    }

    fn send(&mut self, message: &[u8]) -> Result<(), std::io::Error> {
        for line in message.lines() {
            let line = line.unwrap();
            self.stream.write("data: ".as_bytes())?;
            self.stream.write(line.as_bytes())?;
            self.stream.write("\r\n".as_bytes())?;
            eprintln!("Sent message to {:?}: {}", self.stream.peer_addr(),format!("data: {}", line));
        }
        self.stream.write("\r\n\r\n".as_bytes())?; // End of message
        std::io::Write::flush(&mut self.stream)?;
        Ok(())
    }
}

struct Session {
    stream: Option<TcpStream>,
    proxy: Arc<Mutex<TransitProxy>>,
    active_session: Arc<Mutex<Option<MessageQueue>>>,
}

impl Session {
    fn new(stream: std::net::TcpStream, proxy: Arc<Mutex<TransitProxy>>, active_session: Arc<Mutex<Option<MessageQueue>>>) -> Self {
        Session {
            stream: Some(stream),
            proxy,
            active_session,
        }
    }

    fn run(&mut self) {
        let mut parser = HTTPParser::new();
        let mut read_buffer = vec![0; 1024]; // Buffer for reading data
        loop {
            match self.stream.as_ref().unwrap().read(&mut read_buffer) {
                Ok(0) => break, // connection closed
                Ok(n) => {
                    parser.push(&read_buffer[..n]);
                }
                Err(e) => {
                    eprintln!("Error reading from stream: {}", e);
                    break;
                }
            }
            match parser.pop() {
                HTTPParseResult::SSE => {
                    //begin response
                    let response = b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n";
                    self.stream.as_mut().unwrap().write(response).expect("Failed to write to stream");
                    std::io::Write::flush(self.stream.as_mut().unwrap()).expect("Failed to flush stream");
                    //set up the message queue
                    let message_queue = MessageQueue::new(self.stream.take().unwrap());
                    self.active_session.lock().unwrap().replace(message_queue);
                    return; //promoted to active session
                }
                HTTPParseResult::NotFound => {
                    let response = b"HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: 13\r\n\r\n404 Not Found";
                    self.stream.as_mut().unwrap().write_all(response).expect("Failed to write 404 response");
                    std::io::Write::flush(self.stream.as_mut().unwrap()).expect("Failed to flush stream");
                    eprintln!("Sent 404 Not Found response");
                    //continue to next request
                }
                HTTPParseResult::Post(body) => {
                    self.handle_body(&body);
                    //continue to next request
                }
                HTTPParseResult::NotReady => {
                    // continue to read more data
                }
                HTTPParseResult::Rejected(reason) => {
                    let response = format!("HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}", reason.len(), reason);
                    self.stream.as_mut().unwrap().write_all(response.as_bytes()).expect("Failed to write 400 response");
                    std::io::Write::flush(self.stream.as_mut().unwrap()).expect("Failed to flush stream");
                    eprintln!("Sent 400 Bad Request response: {}", reason);
                    //continue to next request
                }
                HTTPParseResult::Websocket(info) => {
                    //https://datatracker.ietf.org/doc/html/rfc6455#section-1.3
                    //honestly the accept field is ridiculous
                    let concat = format!("{}{}", info.key, "258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
                    use sha1::Digest;
                    let mut hasher = sha1::Sha1::default();
                    hasher.update(concat.as_bytes());
                    let hash = hasher.finalize();
                    let accept = base64::prelude::BASE64_STANDARD.encode(&hash);
                    let response = format!(
                        "HTTP/1.1 101 Switching Protocols\r\n\
                         Upgrade: websocket\r\n\
                         Connection: Upgrade\r\n\
                         Sec-WebSocket-Accept: {accept}\r\n\
                         \r\n",
                        accept = accept
                    );
                    self.stream.as_mut().unwrap().write_all(response.as_bytes()).unwrap();
                    std::io::Write::flush(self.stream.as_mut().unwrap()).expect("Failed to flush stream");
                    eprintln!("Sent 101 Switching Protocols upgrade");
                    //take stream
                    let stream = self.stream.take().unwrap();
                    let addr = format!("{}", stream.peer_addr().unwrap());

                    self.proxy.lock().unwrap().change_accept(Some(WebSocketOrStream::WebSocket(WebsocketStream::new(stream))));
                    return; //promoted to transit proxy
                }
            }
        }
    }

    fn handle_body(&mut self, body: &[u8]) {
        let r = self.proxy.lock().unwrap().received_data(body);
        match r {
            Some(response) => {
                let as_bytes = serde_json::to_vec(&response).unwrap();
                let stream = self.stream.as_mut().unwrap();
                // Write the response back to the stream
                stream.write(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: ").unwrap();
                stream.write(as_bytes.len().to_string().as_bytes()).unwrap();
                stream.write(b"\r\n\r\n").unwrap();
                stream.write(&as_bytes).unwrap();
                std::io::Write::flush(stream).expect("Failed to flush stream");
                eprintln!("Sent response: {:?}", String::from_utf8_lossy(&as_bytes));
            }
            None => {
                let stream = self.stream.as_mut().unwrap();
                stream.write("HTTP/1.1 202 Accepted\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".as_bytes()).unwrap();
                std::io::Write::flush(stream).expect("Failed to flush stream");
            }
        }


    }
}

impl Server {
    pub fn new<A: ToSocketAddrs>(addr: A, proxy: TransitProxy) -> Self {
        //listen on a tcp socket
        eprintln!("http: starting MCP server on {}", addr.to_socket_addrs().unwrap().next().unwrap());
        let listener = std::net::TcpListener::bind(addr).unwrap();
        let active_session = Arc::new(Mutex::new(None::<MessageQueue>));
        let move_active_session = active_session.clone();
        proxy.bind(move |notification| {
            let mut sessions = move_active_session.lock().unwrap();
            if let Some(ref mut session) = *sessions {
                let as_bytes = serde_json::to_vec(&notification).unwrap();
                match session.send(&as_bytes) {
                    Ok(_) => {},
                    Err(e) => {
                        eprintln!("http: failed to send notification {:?}: {}", notification, e);
                        //if we fail to send, we should remove the session
                        *sessions = None;
                    }
                }
            } else {
                eprintln!("http: no active session for notification {:?}", notification);
            }
        });
        let proxy = Arc::new(Mutex::new(proxy));


        let move_proxy = proxy.clone();
        std::thread::Builder::new()
            .name("exfiltrate-server".to_string()).spawn(move || {

            loop {
                let (stream,addr) = listener.accept().unwrap();
                Self::on_accept(stream, addr, move_proxy.clone(), active_session.clone());
            }
        }).unwrap();
        Server {
        }
    }

    fn on_accept(stream: std::net::TcpStream, addr: std::net::SocketAddr, proxy: Arc<Mutex<TransitProxy>>, sessions: Arc<Mutex<Option<MessageQueue>>>) {
        //start a new thread to handle the connection
        eprintln!("http: Accepted connection from {}", addr);

        std::thread::Builder::new()
            .name(format!("exfiltrate-server-{}", addr))
            .spawn(move || {
                let mut session = Session::new(stream,proxy, sessions);
                session.run();

            }).unwrap();
    }
}

struct WebsocketFrame {
    data: Vec<u8>,
    //this is required for frames sent from client to server, but forbidden from server to client.
    mask: bool,
}

enum WebsocketFrameError {
    FrameTooShort,
    Rejected(String),
}
impl WebsocketFrame {
    fn new(data: Vec<u8>, mask: bool) -> Self {
        WebsocketFrame { data,mask }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut frame = Vec::new();
        //https://datatracker.ietf.org/doc/html/rfc6455#section-5.2
        //effectively first byte is the opcode,
        const BINARY : u8 = 0b1000_0010; //binary frame, FIN
        frame.push(BINARY); // us
        //second byte is the payload length
        const MASK_ON : u8 = 0b1000000;
        const MASK_OFF : u8 = 0b0000000;
        let mask_current = if self.mask { MASK_ON } else { MASK_OFF };
        if self.data.len() <= 125 {
            frame.push(self.data.len() as u8 | mask_current);
        } else if self.data.len() <= 65535 {
            frame.push(126 | mask_current);
            frame.extend_from_slice(&(self.data.len() as u16).to_be_bytes());
        } else {
            frame.push(127| mask_current);
            frame.extend_from_slice(&(self.data.len() as u64).to_be_bytes());
        }
        if self.mask {
            todo!()
        }
        //add the payload
        frame.extend_from_slice(&self.data);
        frame
    }

    fn from_bytes(bytes: &[u8]) -> Result<(Self, usize), WebsocketFrameError> {
        if bytes.len() == 0 {
            return Err(WebsocketFrameError::FrameTooShort);
        }
        println!("WebsocketFrame::from_bytes: {:?}", bytes);
        if bytes.len() < 2 {
            return Err(WebsocketFrameError::FrameTooShort);
        }
        if bytes[0] & 0b1000_0000 == 0 {
            todo!("FIN bit not handled");
        }
        let opcode = bytes[0] & 0b0111_1111;
        if opcode != 0x2 { //binary frame
            return Err(WebsocketFrameError::Rejected(format!("Invalid opcode: {}", opcode)));
        }
        //second byte is the payload length
        let payload_length = bytes[1] & 0b0111_1111; //mask bit is ignored here
        let mask = bytes[1] & 0b1000_0000 != 0;
        let mask_begin;
        let len;
        if payload_length < 126 {
            len = payload_length as usize;
            mask_begin = 2;
        } else if payload_length == 126 {
            if bytes.len() < 4 {
                return Err(WebsocketFrameError::FrameTooShort);
            }
            let len_bytes = &bytes[2..4];
            len = u16::from_be_bytes(len_bytes.try_into().unwrap()) as usize;
            mask_begin = 4;
        } else {
            if bytes.len() < 10 {
                return Err(WebsocketFrameError::FrameTooShort);
            }
            let len_bytes = &bytes[2..10];
            len = u64::from_be_bytes(len_bytes.try_into().unwrap()) as usize;
            mask_begin = 10;
        }
        let mask_bytes = if mask {
            4
        }
        else {
            0
        };
        let data_begin = mask_begin + mask_bytes;
        if bytes.len() < data_begin + len {
            return Err(WebsocketFrameError::FrameTooShort);
        }
        let mut data = bytes[data_begin..data_begin + len].to_vec();
        //unmask the data
        if mask {
            let masking_key = &bytes[mask_begin..mask_begin + 4];
            for i in 0..data.len() {
                data[i] ^= masking_key[i % 4];
            }
        }
        let frame = WebsocketFrame { data, mask };
        Ok((frame, data_begin + len))
    }
}