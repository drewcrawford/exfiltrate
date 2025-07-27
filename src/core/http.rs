use std::collections::HashMap;
use std::io::{BufRead, Read, Write};
use std::net::ToSocketAddrs;
use serde::Deserialize;
use crate::core::jrpc::Request;

pub struct Server {
}

impl Server {
    pub fn new<A: ToSocketAddrs>(addr: A) -> Self {
        //listen on a tcp socket
        let listener = std::net::TcpListener::bind(addr).unwrap();
        println!("Listening on {}", listener.local_addr().unwrap());
        std::thread::Builder::new()
            .name("exfiltrate-server".to_string()).spawn(move || {

            loop {
                let (stream,addr) = listener.accept().unwrap();
                Self::on_accept(stream, addr);
            }
        }).unwrap();
        Server {

        }
    }

    fn on_accept(stream: std::net::TcpStream, addr: std::net::SocketAddr) {
        //start a new thread to handle the connection
        println!("Accepted connection from {}", addr);
        #[derive(PartialEq)]
        enum ParseState {
            Method,
            Headers,
            Body(usize),
        }
        std::thread::Builder::new()
            .name(format!("exfiltrate-server-{}", addr))
            .spawn(move || {
                //handle the connection
                let mut stream = stream;
                let mut headers_buf = Vec::new();
                let mut read_buffer = [0; 1024];
                let mut parse_state = ParseState::Method;
                let mut body = Vec::new();
                loop {
                    let mut read_slice;
                    match stream.read(&mut read_buffer) {
                        Ok(0) => break, // connection closed
                        Ok(n) => {
                            read_slice = &read_buffer[..n];
                        }
                        Err(e) => {
                            eprintln!("Error reading from stream: {}", e);
                            break;
                        }
                    }
                    if parse_state == ParseState::Method {
                        // If we are in the method state, we expect to read the request line
                        if let Some(pos) = read_slice.iter().position(|&b| b == b'\n') {
                            // We have a complete request line
                            let request_line = &read_slice[..pos];
                            let request_line_str = String::from_utf8_lossy(request_line);
                            println!("Request Line: {}", request_line_str);
                            parse_state = ParseState::Headers;
                            //advance the read_slice
                            read_slice = &read_slice[pos + 1..];
                        } else {
                            // We don't have a complete request line yet, continue reading
                            continue;
                        }
                    }
                    if parse_state == ParseState::Headers {
                        //search for '\r\n\r\n' to find the end of headers
                        if let Some(pos) = read_slice.windows(4).position(|window| window == b"\r\n\r\n") {
                            // We have a complete header block
                            let headers = &read_slice[..pos];
                            headers_buf.extend_from_slice(headers);
                            let headers_str = String::from_utf8_lossy(&headers_buf);
                            // println!("Headers: {}", headers_str);
                            //find content-length header

                            let mut content_length = None;
                            for line in headers_str.lines() {
                                if let Some((key, value)) = line.split_once(": ") {
                                    if key.eq_ignore_ascii_case("Content-Length") {
                                        content_length = Some(value.parse::<usize>().unwrap());
                                    }
                                }
                            }


                            parse_state = ParseState::Body(content_length.expect("content-length header not found"));
                            //advance the read_slice
                            read_slice = &read_slice[pos + 4..];
                        } else {
                            // We don't have a complete header block yet, continue reading
                            headers_buf.extend_from_slice(read_slice);
                            continue;
                        }
                    }
                    if let ParseState::Body(content_length) = parse_state {
                        // We are in the body state, we expect to read the body
                        if read_slice.len() >= content_length {
                            // We have a complete body
                            body.extend_from_slice(&read_slice[..content_length]);
                            let body_str = String::from_utf8_lossy(&body);
                            println!("Body: {}", body_str);
                            // Parse the body as a JSON-RPC request
                            let parse_request: Result<Request,_> = serde_json::from_slice(&body);
                            match parse_request {
                                Ok(request) => {
                                    //dispatch
                                    let response = crate::core::mcp::dispatch(request);
                                    //send response
                                    let response_bytes = serde_json::to_vec(&response).expect("Failed to serialize JSON-RPC response");
                                    stream.write("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: ".as_bytes()).unwrap();
                                    stream.write(response_bytes.len().to_string().as_bytes()).unwrap();
                                    stream.write("\r\n\r\n".as_bytes()).unwrap();
                                    stream.write(&response_bytes).unwrap();
                                    stream.flush().unwrap();
                                    // println!("Response sent to client: {:?}", response);
                                }
                                Err(e) => {
                                    //try parsing as a notification
                                    let parse_notification: crate::core::jrpc::Notification = serde_json::from_str(&body_str).expect("Failed to parse JSON-RPC notification");
                                    println!("Parsed notification: {:?}", parse_notification);
                                    //write a 200 OK response
                                    stream.write("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".as_bytes()).unwrap();
                                }
                            }

                            // Reset for next request
                            headers_buf.clear();
                            body.clear();
                            parse_state = ParseState::Method;
                        } else {
                            // We don't have a complete body yet, continue reading
                            body.extend_from_slice(read_slice);
                        }
                    }
                }

            }).unwrap();
    }
}

#[cfg(test)] mod tests {
    #[test] fn test_server() {
        let server = super::Server::new("127.0.0.1:1984");
        std::thread::sleep(std::time::Duration::from_secs(1000));
    }
}