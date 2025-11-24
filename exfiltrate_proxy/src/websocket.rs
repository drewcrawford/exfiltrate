use WebsocketFrameError::Rejected;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::mem::MaybeUninit;
use std::net::{TcpListener, TcpStream};

/// Starts the WebSocket server on 127.0.0.1:1338.
///
/// This server listens for connections from the WebAssembly application running in the browser.
/// It returns a pair of channels:
/// *   `Sender`: For sending messages to the WebSocket client.
/// *   `Receiver`: For receiving messages from the WebSocket client.
pub fn open_websocket() -> (
    std::sync::mpsc::Sender<Vec<u8>>,
    std::sync::mpsc::Receiver<Vec<u8>>,
) {
    let (to_ws, mut to_ws_recv) = std::sync::mpsc::channel();
    let (mut from_ws_send, from_ws) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("websocket".to_string())
        .spawn(move || {
            let server = TcpListener::bind("127.0.0.1:1338").unwrap();
            for stream in server.incoming() {
                match stream {
                    Ok(stream) => {
                        eprintln!(
                            "New WebSocket connection established: {:?}",
                            stream.peer_addr()
                        );
                        do_stream(stream, &mut to_ws_recv, &mut from_ws_send);
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                    }
                }
            }
        })
        .unwrap();
    (to_ws, from_ws)
}

fn do_stream(
    stream: TcpStream,
    send_data_receiver: &mut std::sync::mpsc::Receiver<Vec<u8>>,
    from_ws_send: &mut std::sync::mpsc::Sender<Vec<u8>>,
) {
    let socket: Websocket = match do_http_stream(stream) {
        None => return,
        Some(_socket) => _socket,
    };
    do_ws_stream(socket, send_data_receiver, from_ws_send);
}

fn do_ws_stream(
    stream: Websocket,
    send_data_receiver: &mut std::sync::mpsc::Receiver<Vec<u8>>,
    from_ws_send: &mut std::sync::mpsc::Sender<Vec<u8>>,
) {
    // We need to split into read and write threads to support full duplex
    let mut write_stream = stream.tcp.try_clone().expect("Failed to clone TCP stream");

    let from_ws_send = from_ws_send.clone();
    let mut reader_stream = stream; // move stream to thread

    std::thread::spawn(move || {
        loop {
            match reader_stream.read() {
                Ok(reply) => {
                    if from_ws_send.send(reply).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("WebSocket read error: {}", e);
                    break;
                }
            }
        }
    });

    // Write loop (main thread)
    loop {
        // We still need to detect if the socket is dead, but `write_all` will fail eventually.
        // Also we need to know if the reader thread died?
        // For now, let's just loop.
        match send_data_receiver.recv() {
            Ok(data) => {
                // Construct frame manually since `Websocket::send` is on the moved object
                // We can duplicate the `send` logic here or make it a static method/helper.
                // Let's copy the logic from `Websocket::send` but using the raw stream.
                let frame = WebsocketFrame::new(data, false);
                let bytes = frame.to_bytes();
                if let Err(e) = write_stream.write_all(&bytes) {
                    eprintln!("WebSocket write error: {}", e);
                    break;
                }
            }
            Err(_) => {
                eprintln!("Channel disconnected");
                break;
            }
        }
    }
}

fn do_http_stream(stream: TcpStream) -> Option<Websocket> {
    let mut bytes: [MaybeUninit<u8>; 1024] = [MaybeUninit::uninit(); 1024];
    let mut http_parse = HTTPParser::new();
    loop {
        let data = unsafe {
            let u8_slice =
                std::slice::from_raw_parts_mut(bytes.as_mut_ptr() as *mut u8, bytes.len());
            let read = (&stream).read(u8_slice);
            match read {
                Ok(data) => std::slice::from_raw_parts_mut(bytes.as_mut_ptr() as *mut u8, data),
                Err(e) => {
                    eprintln!("Read error: {:?}", e);
                    return None;
                }
            }
        };
        http_parse.push(data);
        match http_parse.pop() {
            HTTPParseResult::NotReady => {}
            HTTPParseResult::Rejected(reason) => {
                eprintln!("Rejecting due to {:?}", reason);
                return None;
            }
            HTTPParseResult::NotFound => {
                eprintln!("404 not found");
                return None;
            }
            HTTPParseResult::Websocket(websocket) => {
                eprintln!("Got websocket {:?}", websocket);
                //https://datatracker.ietf.org/doc/html/rfc6455#section-1.3
                //honestly the accept field is ridiculous
                let concat = format!(
                    "{}{}",
                    websocket.key, "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
                );
                use sha1::Digest;
                let mut hasher = sha1::Sha1::default();
                hasher.update(concat.as_bytes());
                let hash = hasher.finalize();
                use base64::Engine;
                let accept = base64::prelude::BASE64_STANDARD.encode(hash);
                let response = format!(
                    "HTTP/1.1 101 Switching Protocols\r\n\
                         Upgrade: websocket\r\n\
                         Connection: Upgrade\r\n\
                         Sec-WebSocket-Accept: {accept}\r\n\
                         \r\n",
                    accept = accept
                );
                use std::io::Write;

                (&stream).write_all(response.as_bytes()).unwrap();
                (&stream).flush().expect("Failed to flush stream");
                eprintln!("Sent 101 Switching Protocols upgrade");
                let mut frame_parser = WebsocketFrameParser::new();
                frame_parser.send_data(&websocket.leftover_bytes);
                return Some(Websocket {
                    frame_parser,
                    tcp: stream,
                });
            }
        }
    }
}

enum HTTPParseResult {
    NotReady,
    Rejected(String),
    NotFound,
    Websocket(WebsocketInfo),
}
#[derive(Debug)]
struct WebsocketInfo {
    key: String,
    leftover_bytes: Vec<u8>,
}

enum WebsocketFrameError {
    TooShort,
    Rejected(String),
    Closed,
}

struct Websocket {
    frame_parser: WebsocketFrameParser,
    tcp: TcpStream,
}

impl Websocket {
    fn read(&mut self) -> Result<Vec<u8>, String> {
        let start_time = std::time::Instant::now();
        let mut waiting_message_printed = false;
        let mut last_progress_print = std::time::Instant::now();

        // Set read timeout for periodic progress checks
        self.tcp
            .set_read_timeout(Some(std::time::Duration::from_millis(100)))
            .ok();

        loop {
            //see if we can parse a frame with no read
            if let Ok(frame) = self.try_parse_frame()
                && !frame.is_empty()
            {
                // Restore no timeout
                self.tcp.set_read_timeout(None).ok();
                return Ok(frame);
            }

            // Check if we should print "Waiting for reply..."
            let frames_received = self.frame_parser.frames_received();
            if !waiting_message_printed
                && frames_received == 0
                && start_time.elapsed().as_secs() >= 5
            {
                eprintln!("Waiting for reply...");
                waiting_message_printed = true;
            }

            // Print progress for multi-frame messages
            if frames_received > 0 && last_progress_print.elapsed().as_millis() > 100 {
                use std::io::Write;
                eprint!("\rReceived frame {} of ?", frames_received);
                std::io::stderr().flush().ok();
                last_progress_print = std::time::Instant::now();
            }

            //if we can't parse a frame, we need to read more data
            let mut buf: [MaybeUninit<u8>; 1024] = [MaybeUninit::uninit(); 1024];
            let data = unsafe {
                let buf_raw =
                    std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, buf.len());
                match self.tcp.read(buf_raw) {
                    Ok(e) => std::slice::from_raw_parts_mut(buf_raw.as_mut_ptr(), e),
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut =>
                    {
                        // Timeout - continue loop to check progress and retry
                        continue;
                    }
                    Err(e) => {
                        return Err(e.to_string());
                    }
                }
            };
            //add data to buf
            self.frame_parser.send_data(data);
        }
    }
    fn try_parse_frame(&mut self) -> Result<Vec<u8>, WebsocketFrameError> {
        //try to parse a frame
        // eprintln!("try_parse_frame: stream_buf len={}", self.tcp_layer_buf.len());
        match self.frame_parser.parse() {
            Err(WebsocketFrameError::TooShort) => {
                Ok(Vec::new()) //not enough data to parse a frame
            }
            Err(WebsocketFrameError::Rejected(reason)) => {
                eprintln!("WebSocket Frame Rejected: {}", reason);
                self.frame_parser = WebsocketFrameParser::new();
                Err(WebsocketFrameError::Rejected(reason))
            }
            Err(WebsocketFrameError::Closed) => Err(WebsocketFrameError::Closed),
            Ok(frame) => Ok(frame.data),
        }
    }
}

struct WebsocketFrameParser {
    parsed_data: Option<Vec<u8>>,
    unparsed_data: Vec<u8>,
    /// Number of frames received so far in this message
    frames_received: usize,
}

impl WebsocketFrameParser {
    fn new() -> WebsocketFrameParser {
        WebsocketFrameParser {
            parsed_data: Some(Vec::new()),
            unparsed_data: Vec::new(),
            frames_received: 0,
        }
    }

    fn send_data(&mut self, data: &[u8]) {
        self.unparsed_data.extend_from_slice(data);
    }

    /// Returns the number of frames received so far
    fn frames_received(&self) -> usize {
        self.frames_received
    }

    fn parse(&mut self) -> Result<WebsocketFrame, WebsocketFrameError> {
        //need to reassemble multiple frames, stopping on FIN bit
        let parsed_data = self.parsed_data.as_mut().unwrap();
        loop {
            if self.unparsed_data.len() < 2 {
                return Err(WebsocketFrameError::TooShort);
            }
            let is_fin = self.unparsed_data[0] & 0b1000_0000 != 0;
            let opcode = self.unparsed_data[0] & 0b0111_1111;
            if opcode == 0x8
            /* close frame */
            {
                return Err(WebsocketFrameError::Closed);
            }
            if opcode != 0x2 /* binary frame*/ && opcode != 0
            /* continuation */
            {
                return Err(Rejected(format!("Opcode not supported: {:?}", opcode)));
            }
            //second byte is the payload length
            let payload_length = self.unparsed_data[1] & 0b0111_1111; //mask bit is ignored here
            let mask = self.unparsed_data[1] & 0b1000_0000 != 0;
            let mask_begin;
            let len;
            if payload_length < 126 {
                len = payload_length as usize;
                mask_begin = 2;
            } else if payload_length == 126 {
                if self.unparsed_data.len() < 4 {
                    return Err(WebsocketFrameError::TooShort);
                }
                let len_bytes = &self.unparsed_data[2..4];
                len = u16::from_be_bytes(len_bytes.try_into().unwrap()) as usize;
                mask_begin = 4;
            } else {
                if self.unparsed_data.len() < 10 {
                    return Err(WebsocketFrameError::TooShort);
                }
                let len_bytes = &self.unparsed_data[2..10];
                len = u64::from_be_bytes(len_bytes.try_into().unwrap()) as usize;
                mask_begin = 10;
            }
            let mask_bytes = if mask { 4 } else { 0 };
            let data_begin = mask_begin + mask_bytes;

            if self.unparsed_data.len() < data_begin + len {
                return Err(WebsocketFrameError::TooShort);
            }

            // Frame successfully parsed, increment counter
            self.frames_received += 1;
            let prior_out_len = parsed_data.len();
            parsed_data.extend_from_slice(&self.unparsed_data[data_begin..data_begin + len]);

            //unmask the data, in-place
            let data_region = &mut parsed_data[prior_out_len..prior_out_len + len];
            if mask {
                let masking_key = &self.unparsed_data[mask_begin..mask_begin + 4];
                for i in 0..data_region.len() {
                    data_region[i] ^= masking_key[i % 4];
                }
            }
            self.unparsed_data.drain(0..data_begin + len);
            if is_fin {
                eprintln!("\nFin message DONE with {} bytes", parsed_data.len());
                let old_data = self.parsed_data.take().unwrap();
                self.parsed_data = Some(Vec::new());
                self.frames_received = 0; // Reset for next message
                return Ok(WebsocketFrame {
                    data: old_data,
                    mask,
                });
            } else {
                //continue
            }
        }
    }
}
struct WebsocketFrame {
    data: Vec<u8>,
    //this is required for frames sent from client to server, but forbidden from server to client.
    mask: bool,
}
impl WebsocketFrame {
    fn new(data: Vec<u8>, mask: bool) -> Self {
        WebsocketFrame { data, mask }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut frame = Vec::new();
        //https://datatracker.ietf.org/doc/html/rfc6455#section-5.2
        //effectively first byte is the opcode,
        const BINARY: u8 = 0b1000_0010; //binary frame, FIN
        frame.push(BINARY); // us
        //second byte is the payload length
        const MASK_ON: u8 = 0b10000000;
        const MASK_OFF: u8 = 0b0000000;
        let mask_current = if self.mask { MASK_ON } else { MASK_OFF };
        if self.data.len() <= 125 {
            frame.push(self.data.len() as u8 | mask_current);
        } else if self.data.len() <= 65535 {
            frame.push(126 | mask_current);
            frame.extend_from_slice(&(self.data.len() as u16).to_be_bytes());
        } else {
            frame.push(127 | mask_current);
            frame.extend_from_slice(&(self.data.len() as u64).to_be_bytes());
        }
        if self.mask {
            todo!()
        }
        //add the payload
        frame.extend_from_slice(&self.data);
        frame
    }

    #[allow(dead_code)]
    fn from_bytes(orig_bytes: &[u8]) -> Result<(Self, usize), WebsocketFrameError> {
        //need to reassemble multiple frames, stopping on FIN bit
        let mut out = Vec::new();
        let mut used_bytes = 0;
        loop {
            //update bytes to point to correct location
            let bytes = &orig_bytes[used_bytes..];
            eprintln!(
                "orig_bytes len: {} bytes_len {}",
                orig_bytes.len(),
                bytes.len()
            );
            if bytes.len() < 2 {
                return Err(WebsocketFrameError::TooShort);
            }
            let is_fin = bytes[0] & 0b1000_0000 != 0;
            let opcode = bytes[0] & 0b0111_1111;
            eprintln!("First byte: {}", bytes[0]);
            if opcode != 0x2 /* binary frame*/ && opcode != 0
            /* continuation */
            {
                return Err(Rejected(format!("Opcode not supported: {:?}", opcode)));
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
                    return Err(WebsocketFrameError::TooShort);
                }
                let len_bytes = &bytes[2..4];
                len = u16::from_be_bytes(len_bytes.try_into().unwrap()) as usize;
                mask_begin = 4;
            } else {
                if bytes.len() < 10 {
                    return Err(WebsocketFrameError::TooShort);
                }
                let len_bytes = &bytes[2..10];
                len = u64::from_be_bytes(len_bytes.try_into().unwrap()) as usize;
                mask_begin = 10;
            }
            eprintln!("Length calculated at {:?}", len);
            let mask_bytes = if mask { 4 } else { 0 };
            let data_begin = mask_begin + mask_bytes;
            eprintln!("data begin at {:?}", data_begin);
            if bytes.len() < data_begin + len {
                return Err(WebsocketFrameError::TooShort);
            }
            let prior_out_len = out.len();
            out.extend_from_slice(&bytes[data_begin..data_begin + len]);

            //unmask the data, in-place
            let data_region = &mut out[prior_out_len..prior_out_len + len];
            if mask {
                let masking_key = &bytes[mask_begin..mask_begin + 4];
                for i in 0..data_region.len() {
                    data_region[i] ^= masking_key[i % 4];
                }
            }

            used_bytes += data_begin;
            used_bytes += len;
            if is_fin {
                eprintln!("Fin message DONE with {} bytes", out.len());
                return Ok((WebsocketFrame { data: out, mask }, used_bytes));
            } else {
                eprintln!("Continuing non-fin message");
            }
        }
    }
}

struct HTTPParser {
    buf: Vec<u8>,
}
impl HTTPParser {
    fn new() -> Self {
        HTTPParser { buf: Vec::new() }
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
            } else if line == b"\r" {
                //blank line indicates end of headers
                pos += 2; //carriage return + newline
                found_blank = true;
                break;
            } else {
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
                return HTTPParseResult::Rejected("No request line found".to_string());
            }
        };

        //get method, url and version
        let mut split_line = request_line.split(|&b| b == b' ');
        let method = match split_line.next() {
            Some(method) => method,
            None => {
                let f = format!(
                    "Invalid request line: {}",
                    String::from_utf8_lossy(request_line)
                );
                self.buf.clear();
                return HTTPParseResult::Rejected(f);
            }
        };
        let url = match split_line.next() {
            Some(url) => url,
            None => {
                let f = format!(
                    "Invalid request line: {}",
                    String::from_utf8_lossy(request_line)
                );
                self.buf.clear();
                return HTTPParseResult::Rejected(f);
            }
        };
        let _ = match split_line.next() {
            Some(version) => version,
            None => {
                let f = format!(
                    "Invalid request line: {}",
                    String::from_utf8_lossy(request_line)
                );
                self.buf.clear();
                return HTTPParseResult::Rejected(f);
            }
        };
        //the rest of the lines are headers
        let mut headers = HashMap::new();
        for line in &http_lines[1..] {
            let mut split = line.splitn(2, |&b| b == b':');
            let key = match split.next() {
                //http headers are case-insensitive, so we convert to lowercase
                Some(key) => String::from_utf8_lossy(key)
                    .trim()
                    .to_lowercase()
                    .to_owned(),
                None => {
                    let f = format!(
                        "Invalid header line: {}",
                        String::from_utf8_lossy(request_line)
                    );
                    self.buf.clear();
                    return HTTPParseResult::Rejected(f);
                }
            };
            let val = match split.next() {
                Some(val) => String::from_utf8_lossy(val).trim().to_owned(),
                None => {
                    let f = format!(
                        "Invalid header line: {}",
                        String::from_utf8_lossy(request_line)
                    );
                    self.buf.clear();
                    return HTTPParseResult::Rejected(f);
                }
            };
            headers.insert(key, val);
        }
        //with that out of the way, let's consider some cases.
        if url != b"/" {
            self.buf.clear();
            return HTTPParseResult::NotFound;
        }
        let _accept_header = headers.get("accept").map(|s| s.as_str()).unwrap_or("");
        if method == b"GET" && headers.get("upgrade").map(|s| s.as_str()) == Some("websocket") {
            let key = headers
                .get("sec-websocket-key")
                .map(|s| s.as_str())
                .unwrap_or("")
                .to_owned();
            HTTPParseResult::Websocket(WebsocketInfo {
                key,
                leftover_bytes: self.buf[pos..].to_vec(),
            })
        } else {
            let f = format!("request {}", String::from_utf8_lossy(&self.buf));
            self.buf.clear();
            HTTPParseResult::Rejected(f)
        }
    }
}
