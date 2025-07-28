use std::collections::HashMap;
use std::io::{BufRead, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, LazyLock, Mutex, Weak};
use serde::Deserialize;
use crate::jrpc::{Request,Response};
use crate::transit::transit_proxy::TransitProxy;


#[derive(PartialEq)]
enum ParseState {
    Method,
    Headers,
    Body(usize),
}


pub struct Server {
    proxy: Arc<Mutex<TransitProxy>>,
    active_sessions: Arc<Mutex<Vec<Mutex<MessageQueue>>>>,
}

pub struct MessageQueue {
    stream: TcpStream,
}

impl MessageQueue {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream,
        }
    }

    pub fn send(&mut self, message: &[u8]) {
        for line in message.lines() {
            let line = line.unwrap();
            self.stream.write("data: ".as_bytes()).unwrap();
            self.stream.write(line.as_bytes()).unwrap();
            self.stream.write("\r\n".as_bytes()).unwrap();
            println!("Sent message to {:?}: {}", self.stream.peer_addr(),format!("data: {}", line));
        }
        self.stream.write("\r\n\r\n".as_bytes()).unwrap(); // End of message
        self.stream.flush().expect("Failed to flush stream");
    }
}

struct Session {
    stream: Option<TcpStream>,
    proxy: Arc<Mutex<TransitProxy>>,
    active_sessions: Arc<Mutex<Vec<Mutex<MessageQueue>>>>,
}

impl Session {
    fn new(stream: std::net::TcpStream, proxy: Arc<Mutex<TransitProxy>>, active_sessions: Arc<Mutex<Vec<Mutex<MessageQueue>>>>) -> Self {
        Session {
            stream: Some(stream),
            proxy,
            active_sessions,
        }
    }

    fn run(&mut self) {
        let mut headers_buf = Vec::new();
        let mut read_buffer = [0; 1024];
        let mut parse_state = ParseState::Method;
        let mut method = None;
        let mut url = None;
        let mut body = Vec::new();
        loop {
            let mut read_slice;
            match self.stream.as_ref().unwrap().read(&mut read_buffer) {
                Ok(0) => break, // connection closed
                Ok(n) => {
                    read_slice = &read_buffer[..n];
                }
                Err(e) => {
                    eprintln!("Error reading from stream: {}", e);
                    break;
                }
            }
            if parse_state == crate::transit::http::ParseState::Method {
                // If we are in the method state, we expect to read the request line
                if let Some(pos) = read_slice.iter().position(|&b| b == b'\n') {
                    // We have a complete request line
                    let request_line = &read_slice[..pos];
                    let request_line_str = String::from_utf8_lossy(request_line);
                    // Parse the request line
                    let (method_str, rest) = request_line_str.split_once(' ').expect("Error parsing request line");
                    let (url_str, _) = rest.split_once(' ').expect("Error parsing request line");
                    method = Some(method_str.to_string());
                    url = Some(url_str.to_string());
                    println!("Method: {method:?}, URL: {url:?}");
                    parse_state = crate::transit::http::ParseState::Headers;
                    //advance the read_slice
                    read_slice = &read_slice[pos + 1..];
                } else {
                    // We don't have a complete request line yet, continue reading
                    continue;
                }
            }
            if parse_state == crate::transit::http::ParseState::Headers {
                //search for '\r\n\r\n' to find the end of headers
                if let Some(pos) = read_slice.windows(4).position(|window| window == b"\r\n\r\n") {
                    // We have a complete header block
                    let headers = &read_slice[..pos];
                    headers_buf.extend_from_slice(headers);
                    println!("Headers: {}", String::from_utf8_lossy(&headers_buf));

                    if method.as_ref().unwrap() == "GET" && url.as_ref().unwrap() == "/" {
                        //begin response
                        let response = b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n";
                        self.stream.as_mut().unwrap().write(response).expect("Failed to write to stream");
                        self.stream.as_mut().unwrap().flush().expect("Failed to flush stream");
                        //set up the message queue
                        let message_queue = MessageQueue::new(self.stream.take().unwrap());
                        self.active_sessions.lock().unwrap().push(Mutex::new(message_queue));
                        return; //promoted to active session
                    }
                    else if url.as_ref().unwrap() != "/" {
                        // other requests return 404
                        let response = b"HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: 13\r\n\r\n404 Not Found";
                        self.stream.as_mut().unwrap().write_all(response).expect("Failed to write 404 response");
                        self.stream.as_mut().unwrap().flush().expect("Failed to flush stream");
                        println!("Sent 404 Not Found response");
                        // Reset for next request
                        headers_buf.clear();
                        body.clear();
                        parse_state = crate::transit::http::ParseState::Method;
                        continue; // continue to the next iteration

                    }
                    //find content-length header

                    let mut content_length = None;
                    for line in headers.lines() {
                        let line = line.unwrap();
                        if let Some((key, value)) = line.split_once(": ") {
                            if key.eq_ignore_ascii_case("Content-Length") {
                                content_length = Some(value.parse::<usize>().unwrap());
                            }
                        }
                    }


                    parse_state = crate::transit::http::ParseState::Body(content_length.expect("content-length header not found"));
                    //advance the read_slice
                    read_slice = &read_slice[pos + 4..];
                } else {
                    // We don't have a complete header block yet, continue reading
                    headers_buf.extend_from_slice(read_slice);
                    continue;
                }
            }
            if let crate::transit::http::ParseState::Body(content_length) = parse_state {
                // We are in the body state, we expect to read the body
                if read_slice.len() >= content_length {
                    // We have a complete body
                    body.extend_from_slice(&read_slice[..content_length]);
                    let body_str = String::from_utf8_lossy(&body);
                    println!("Body: {}", body_str);

                    self.handle_body(&body);

                    // Reset for next request
                    headers_buf.clear();
                    body.clear();
                    parse_state = crate::transit::http::ParseState::Method;
                } else {
                    // We don't have a complete body yet, continue reading
                    body.extend_from_slice(read_slice);
                }
            }
        }
    }

    fn initial_setup(&mut self) {
    }


    fn handle_body(&mut self, body: &[u8]) {
        // Parse the body as a JSON-RPC request
        let parse_request: Result<Request,_> = serde_json::from_slice(&body);

        match parse_request {
            Ok(request) => {
                let request_id = request.id.clone();
                let stream = self.stream.as_mut().unwrap();
                let response = match self.proxy.lock().unwrap().send_request(request.clone()) {
                    Ok(response) => response,
                    Err(e) => {
                        eprintln!("Error sending request to proxy: {}", e);
                        Response::err(crate::jrpc::Error::new(-32001, format!("{e:?}"), None), request_id)
                    }
                };
                let response = serde_json::to_vec(&response).expect("Failed to serialize JSON-RPC response");
                // Write the response back to the stream
                stream.write(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: ").unwrap();
                stream.write(response.len().to_string().as_bytes()).unwrap();
                stream.write(b"\r\n\r\n").unwrap();
                stream.write(&response).unwrap();
                stream.flush().unwrap();
                println!("Sent response: {:?}", String::from_utf8_lossy(&response));
            }
            Err(e) => {

                //try parsing as a notification
                let parse_notification: crate::jrpc::Notification = serde_json::from_slice(&body).expect("Failed to parse JSON-RPC notification");
                println!("Parsed notification: {:?}", parse_notification);
                if parse_notification.method == "notifications/initialized" {
                    self.initial_setup();
                }
                let stream = self.stream.as_mut().unwrap();
                //write a 202 Accepted OK response
                stream.write("HTTP/1.1 202 Accepted\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".as_bytes()).unwrap();
            }
        }
    }
}

impl Server {
    pub fn new<A: ToSocketAddrs>(addr: A, proxy: TransitProxy) -> Self {
        //listen on a tcp socket
        let listener = std::net::TcpListener::bind(addr).unwrap();
        let proxy = Arc::new(Mutex::new(proxy));
        println!("MCP/HTTP Listening on {}", listener.local_addr().unwrap());
        let active_sessions = Arc::new(Mutex::new(Vec::new()));
        let move_proxy = proxy.clone();
        std::thread::Builder::new()
            .name("exfiltrate-server".to_string()).spawn(move || {

            loop {
                let (stream,addr) = listener.accept().unwrap();
                Self::on_accept(stream, addr, move_proxy.clone(), active_sessions.clone());
            }
        }).unwrap();
        Server {
            proxy,
            active_sessions: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn on_accept(stream: std::net::TcpStream, addr: std::net::SocketAddr, proxy: Arc<Mutex<TransitProxy>>, sessions: Arc<Mutex<Vec<Mutex<MessageQueue>>>>) {
        //start a new thread to handle the connection
        println!("Accepted connection from {}", addr);

        std::thread::Builder::new()
            .name(format!("exfiltrate-server-{}", addr))
            .spawn(move || {
                let mut session = Session::new(stream,proxy, sessions);
                session.run();

            }).unwrap();
    }
}

