use std::collections::HashMap;
use std::io::{BufRead, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use crate::transit::transit_proxy::TransitProxy;


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
        }
        else {
            let f = format!("Unsupported method or URL: {} {}", String::from_utf8_lossy(method), String::from_utf8_lossy(url));
            self.buf.clear();
            HTTPParseResult::Rejected(f)
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
        self.stream.flush()?;
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
                    self.stream.as_mut().unwrap().flush().expect("Failed to flush stream");
                    //set up the message queue
                    let message_queue = MessageQueue::new(self.stream.take().unwrap());
                    self.active_session.lock().unwrap().replace(message_queue);
                    return; //promoted to active session
                }
                HTTPParseResult::NotFound => {
                    let response = b"HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: 13\r\n\r\n404 Not Found";
                    self.stream.as_mut().unwrap().write_all(response).expect("Failed to write 404 response");
                    self.stream.as_mut().unwrap().flush().expect("Failed to flush stream");
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
                    self.stream.as_mut().unwrap().flush().expect("Failed to flush stream");
                    eprintln!("Sent 400 Bad Request response: {}", reason);
                    //continue to next request
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
                stream.flush().unwrap();
                eprintln!("Sent response: {:?}", String::from_utf8_lossy(&as_bytes));
            }
            None => {
                let stream = self.stream.as_mut().unwrap();
                stream.write("HTTP/1.1 202 Accepted\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".as_bytes()).unwrap();
                stream.flush().unwrap();
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
        eprintln!("Accepted connection from {}", addr);

        std::thread::Builder::new()
            .name(format!("exfiltrate-server-{}", addr))
            .spawn(move || {
                let mut session = Session::new(stream,proxy, sessions);
                session.run();

            }).unwrap();
    }
}

