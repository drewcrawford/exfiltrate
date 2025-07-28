use std::net::TcpStream;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::Arc;
use std::io::{Write,Read};
use crate::jrpc::{Request, Response};

pub struct Accept {
    bidirectional: crate::bidirectional_proxy::BidirectionalProxy<TcpStream>,
    addr: std::net::SocketAddr,
}


pub struct SharedAccept {
    latest_accept: Option<Accept>,
    buffered_messages: Vec<Box<[u8]>>,
    process_notifications: Box<dyn Fn(crate::jrpc::Notification) + Send + Sync>,
}

pub struct TransitProxy {
    shared_accept: Arc<Mutex<SharedAccept>>,
    message_receiver: std::sync::mpsc::Receiver<crate::jrpc::Response<serde_json::Value>>,

}

#[derive(Debug,thiserror::Error)]
pub enum Error {
    #[error("Not connected to the exfiltrated application")]
    NotConnected,
    #[error("Failed to send message: {0}")]
    TransitError(#[from] crate::bidirectional_proxy::Error),
    #[error("Failed to parse message: {0}")]
    JRPCError(#[from] crate::jrpc::Error),
}

impl TransitProxy {
    pub fn new(

    ) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:1985").unwrap();
        eprintln!("Proxy listening on {}", listener.local_addr().unwrap());
        let shared_accept = Arc::new(Mutex::new(SharedAccept::new()));
        let per_msg_shared_accept = shared_accept.clone();
        let per_thread_shared_accept = shared_accept.clone();
        let (message_sender, message_receiver) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("exfiltrate::TransitProxy".to_string())
            .spawn( move || {
                let stream = listener.accept().unwrap();
                eprintln!("transit_proxy accepted internal_proxy from {}", stream.0.peer_addr().unwrap());
                let bidirectional_proxy = crate::bidirectional_proxy::BidirectionalProxy::new(stream.0, move |msg| {
                    eprintln!("transit_proxy received message: {:?}", String::from_utf8_lossy(&msg));
                    //try parsing as a response
                    let response: Result<crate::jrpc::Response<serde_json::Value>, _> = serde_json::from_slice(&msg);
                    match response {
                        Ok(response) => {
                            message_sender.send(response).unwrap();
                            None // We don't need to send a response back, just notify the receiver
                        }
                        Err(e) => {
                            //try parsing as notification instead
                            let notification: Result<crate::jrpc::Notification, _> = serde_json::from_slice(&msg);
                            match notification {
                                Ok(notification) => {
                                    eprintln!("Received notification: {:?}", notification);
                                    per_msg_shared_accept.lock().unwrap().received_notification(notification);
                                    None
                                }
                                Err(e) => {
                                    panic!("Failed to parse message as response or notification: {}", e);
                                }
                            }
                        }
                    }

                });
                per_thread_shared_accept.lock().unwrap().latest_accept = Some(Accept { bidirectional: bidirectional_proxy, addr: stream.1});

            });
        TransitProxy {
            shared_accept,
            message_receiver,
        }
    }

    pub(crate) fn bind<F>(&self, process_notifications: F)
    where
        F: Fn(crate::jrpc::Notification) + Send + Sync + 'static,
    {
        let mut shared = self.shared_accept.lock().unwrap();
        shared.process_notifications = Box::new(process_notifications);
    }
}

impl TransitProxy {
    pub fn send_request(&mut self, message: crate::jrpc::Request) -> Result<crate::jrpc::Response<serde_json::Value>,Error> {
        // some things we do locally always
        match message.method.as_str() {
            "initialize" => {
                return Ok(initialize(message).erase())
            }
            _ => {}
        }
        let mut shared = self.shared_accept.lock().unwrap();
        let request = serde_json::to_vec(&message).unwrap();
        //some things we do locally IF there's no connection
        match &mut shared.latest_accept {
            Some(accept) => {
                accept.bidirectional.send(&request)?;
                eprintln!("Request sent to remote accept: {:?} {:?}", accept.addr, String::from_utf8_lossy(&request));
                eprintln!("Waiting for response to request: {:?}", message);
                let msg = self.message_receiver.recv().unwrap();
                assert!(msg.id == message.id, "Received response with mismatched ID: expected {:?}, got {:?}", message.id, msg.id);
                eprintln!("Received response: {:?}", msg);
                Ok(msg)
            }
            None => return Self::local_fallback(message),
        }
    }

    fn local_fallback(message: crate::jrpc::Request) -> Result<crate::jrpc::Response<serde_json::Value>, Error> {
        eprintln!("Local fallback for request: {:?}", &message);
        match message.method.as_str() {
            "tools/list" => {
                let result = crate::tools::list_local();
                Ok(Response::new(result, message.id).erase())
            }
            "tools/call" => {
                let params: crate::tools::ToolCallParams = serde_json::from_value(message.params.unwrap()).unwrap();
                let result = crate::tools::call_local(params);
                match result {
                    Ok(response) => Ok(Response::new(response, message.id).erase()),
                    Err(e) => Err(e.into()),
                }
            }
            _ => {
                eprintln!("No connection available, cannot send request: {:?}", message);
                return Err(Error::NotConnected);
            }
        }
    }

    pub fn send_notification(&mut self, message: crate::jrpc::Notification) {
        todo!();
    }
}

impl SharedAccept {
    fn received_notification(&mut self, notification: crate::jrpc::Notification) {
        (self.process_notifications)(notification);
    }

    fn new() -> Self {
        SharedAccept {
            latest_accept: None,
            buffered_messages: Vec::new(),
            process_notifications: Box::new(|_notification| {}),
        }
    }
}

fn initialize(request: Request) -> Response<InitializeResult> {
    Response::new(InitializeResult::new(), request.id)
}

#[derive(Debug, serde::Serialize)]
struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    protocol_version: String,
    capabilities: HashMap<String, HashMap<String, serde_json::Value>>,
    #[serde(rename = "serverInfo")]
    server_info: HashMap<String, serde_json::Value>,

}

impl InitializeResult {
    fn new() -> Self {
        let mut server_info = HashMap::new();
        server_info.insert("name".to_string(), "exfiltrate".into());
        server_info.insert("version".to_string(), "0.1.0".into());

        let mut capabilities = HashMap::new();
        let mut tool_capabilities = HashMap::new();
        tool_capabilities.insert("listChanged".to_string(), true.into());
        capabilities.insert("tools".to_string(), tool_capabilities);
        InitializeResult {
            protocol_version: "2025-06-18".to_string(),
            capabilities,
            server_info,

        }
    }
}