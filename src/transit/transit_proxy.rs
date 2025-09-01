use crate::jrpc::{Request, Response};
use crate::tools::{ToolCallParams, ToolCallResponse, ToolList};
use crate::transit::http::{ReadWebSocketOrStream, WriteWebSocketOrStream};
use crate::transit::log_proxy::LogProxy;
use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::Mutex;

/// Represents an accepted connection to the transit proxy.
///
/// This struct encapsulates a bidirectional communication channel
/// and the address information of the connected peer.
#[derive(Debug)]
pub struct Accept {
    bidirectional: crate::bidirectional_proxy::BidirectionalProxy,
    addr: String,
}

impl Accept {
    /// Creates a new accepted connection.
    ///
    /// # Arguments
    ///
    /// * `bidirectional` - The bidirectional proxy for message communication
    /// * `addr` - String representation of the peer's address
    pub fn new(
        bidirectional: crate::bidirectional_proxy::BidirectionalProxy,
        addr: String,
    ) -> Self {
        Accept {
            bidirectional,
            addr,
        }
    }
}

/// Thread-safe container for managing accepted connections and notification handling.
///
/// This struct is shared across threads to coordinate connection state
/// and notification processing.
pub struct SharedAccept {
    latest_accept: Option<Accept>,
    process_notifications: Box<dyn Fn(crate::jrpc::Notification) + Send + Sync>,
}

/// Core proxy component that manages connections and routes JSON-RPC messages.
///
/// The `TransitProxy` acts as an intermediary between clients and target applications,
/// intercepting and potentially modifying JSON-RPC communication. It provides:
///
/// - Connection management for both TCP and WebSocket protocols
/// - Message routing between clients and targets
/// - Tool injection to augment target capabilities
/// - Fallback handling when no target is connected
///
/// # Example
/// ```
/// # #[cfg(feature = "transit")]
/// # {
/// use exfiltrate::transit::transit_proxy::TransitProxy;
///
/// // Create a new transit proxy
/// let mut proxy = TransitProxy::new();
///
/// // The proxy listens on 127.0.0.1:1985 for internal connections
/// // It can handle JSON-RPC requests even without a target connection
/// # }
/// ```
pub struct TransitProxy {
    shared_accept: Arc<Mutex<SharedAccept>>,
    message_receiver: std::sync::mpsc::Receiver<crate::jrpc::Response<serde_json::Value>>,
    message_sender: std::sync::mpsc::Sender<crate::jrpc::Response<serde_json::Value>>,
}

/// Errors that can occur during transit proxy operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No target application is currently connected to the proxy
    #[error("Not connected to the exfiltrated application")]
    NotConnected,
    /// Failed to send message through the bidirectional proxy
    #[error("Failed to send message: {0}")]
    TransitError(#[from] crate::bidirectional_proxy::Error),
    /// Failed to parse JSON-RPC message
    #[error("Failed to parse message: {0}")]
    JRPCError(#[from] crate::jrpc::Error),
}

fn bidi_fn(
    message_sender: &std::sync::mpsc::Sender<crate::jrpc::Response<serde_json::Value>>,
    per_msg_shared_accept: &Arc<Mutex<SharedAccept>>,
    msg: Box<[u8]>,
) -> Option<Box<[u8]>> {
    eprintln!(
        "transit_proxy received message: {:?}",
        String::from_utf8_lossy(&msg)
    );
    //try parsing as a response
    let response: Result<crate::jrpc::Response<serde_json::Value>, _> =
        serde_json::from_slice(&msg);
    match response {
        Ok(response) => {
            message_sender.send(response).unwrap();
            None // We don't need to send a response back, just notify the receiver
        }
        Err(_) => {
            //try parsing as notification instead
            let notification: Result<crate::jrpc::Notification, _> = serde_json::from_slice(&msg);
            match notification {
                Ok(notification) => {
                    eprintln!("Received notification: {:?}", notification);
                    per_msg_shared_accept
                        .lock()
                        .unwrap()
                        .received_notification(notification);
                    None
                }
                Err(e) => {
                    panic!("Failed to parse message as response or notification: {}", e);
                }
            }
        }
    }
}

impl TransitProxy {
    /// Creates a new transit proxy instance.
    ///
    /// This starts a TCP listener on `127.0.0.1:1985` that waits for
    /// internal proxy connections. The proxy runs in a background thread
    /// and can handle both TCP and WebSocket connections.
    ///
    /// # Example
    /// ```
    /// # #[cfg(feature = "transit")]
    /// # {
    /// use exfiltrate::transit::transit_proxy::TransitProxy;
    ///
    /// let proxy = TransitProxy::new();
    /// // Proxy is now listening on 127.0.0.1:1985
    /// # }
    /// ```
    pub fn new() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:1985").unwrap();
        eprintln!("transit: listening on {}", listener.local_addr().unwrap());
        let shared_accept = Arc::new(Mutex::new(SharedAccept::new()));
        let per_msg_shared_accept = shared_accept.clone();
        let per_thread_shared_accept = shared_accept.clone();

        let (message_sender, message_receiver) = std::sync::mpsc::channel();
        let per_msg_message_sender = message_sender.clone();
        std::thread::Builder::new()
            .name("exfiltrate::TransitProxy".to_string())
            .spawn(move || {
                let stream = listener.accept().unwrap();
                eprintln!(
                    "transit_proxy accepted internal_proxy from {}",
                    stream.0.peer_addr().unwrap()
                );
                let split = (stream.0.try_clone().unwrap(), stream.0);
                let write_stream = WriteWebSocketOrStream::Stream(split.0);
                let read_stream = ReadWebSocketOrStream::Stream(split.1);

                let bidirectional_proxy = crate::bidirectional_proxy::BidirectionalProxy::new(
                    write_stream,
                    read_stream,
                    move |msg| bidi_fn(&per_msg_message_sender, &per_msg_shared_accept, msg),
                );
                let peer_string = format!("{}", stream.1);
                per_thread_shared_accept.lock().unwrap().latest_accept = Some(Accept {
                    bidirectional: bidirectional_proxy,
                    addr: peer_string,
                });
            })
            .unwrap();
        TransitProxy {
            shared_accept,
            message_receiver,
            message_sender,
        }
    }

    /// Binds a notification handler to process incoming notifications.
    ///
    /// The handler will be called for each notification received from
    /// the target application, except for special notifications that
    /// are handled internally (like logwise records).
    ///
    /// # Arguments
    ///
    /// * `process_notifications` - Function to handle notifications
    pub(crate) fn bind<F>(&self, process_notifications: F)
    where
        F: Fn(crate::jrpc::Notification) + Send + Sync + 'static,
    {
        let mut shared = self.shared_accept.lock().unwrap();
        shared.process_notifications = Box::new(process_notifications);
    }

    /// Changes the current accepted connection.
    ///
    /// This is used internally to upgrade connections from TCP to WebSocket
    /// or to replace the current connection with a new one.
    ///
    /// # Arguments
    ///
    /// * `new_accept` - Optional tuple of write and read transports
    pub(crate) fn change_accept(
        &self,
        new_accept: Option<(WriteWebSocketOrStream, ReadWebSocketOrStream)>,
    ) {
        let bidi = match new_accept {
            Some(ws) => {
                let move_sender = self.message_sender.clone();
                let move_shared_accept = self.shared_accept.clone();
                let bidirectional =
                    crate::bidirectional_proxy::BidirectionalProxy::new(ws.0, ws.1, move |msg| {
                        let move_sender = move_sender.clone();
                        bidi_fn(&move_sender, &move_shared_accept, msg)
                    });
                Some(Accept::new(bidirectional, "WebSocket".to_string()))
            }
            None => None,
        };
        let mut shared = self.shared_accept.lock().unwrap();
        shared.latest_accept = bidi;
        eprintln!("transit: Changed accept to {:?}", shared.latest_accept);
    }
}

impl TransitProxy {
    /// Processes incoming data from a client.
    ///
    /// Parses the data as either a JSON-RPC request or notification.
    /// Requests are forwarded to the target (if connected) or handled
    /// locally for certain methods. Returns a response for requests,
    /// or `None` for notifications.
    ///
    /// # Arguments
    ///
    /// * `data` - Raw bytes containing JSON-RPC message
    ///
    /// # Returns
    ///
    /// * `Some(Response)` for requests
    /// * `None` for notifications
    pub fn received_data(&mut self, data: &[u8]) -> Option<Response<serde_json::Value>> {
        let parse_request: Result<Request, _> = serde_json::from_slice(&data);
        match parse_request {
            Ok(request) => {
                let request_id = request.id.clone();
                let response = self.send_request(request);

                let r = match response {
                    Ok(response) => response,
                    Err(e) => {
                        eprintln!("Error sending request to proxy: {}", e);
                        Response::err(crate::jrpc::Error::from_error(e), request_id)
                    }
                };
                Some(r)
            }
            Err(_) => {
                //try parsing as a notification
                let parse_notification: crate::jrpc::Notification =
                    serde_json::from_slice(&data).expect("Failed to parse JSON-RPC notification");
                eprintln!("transit: Parsed notification: {:?}", parse_notification);
                if parse_notification.method == "notifications/initialized" {
                    self.initial_setup();
                }
                None
            }
        }
    }
    /// Sends a JSON-RPC request to the target application.
    ///
    /// Some requests are handled locally (like "initialize"), while others
    /// are forwarded to the connected target. If no target is connected,
    /// falls back to local handling for supported methods.
    ///
    /// This method also handles tool injection, adding proxy-only tools
    /// to the responses from the target.
    ///
    /// # Arguments
    ///
    /// * `message` - The JSON-RPC request to send
    ///
    /// # Returns
    ///
    /// * `Ok(Response)` on success
    /// * `Err(Error)` if sending fails or no target is connected
    pub fn send_request(
        &mut self,
        message: crate::jrpc::Request,
    ) -> Result<crate::jrpc::Response<serde_json::Value>, Error> {
        // some things we do locally always
        match message.method.as_str() {
            "initialize" => return Ok(initialize(message).erase()),
            _ => {}
        }
        let mut shared = self.shared_accept.lock().unwrap();
        let request = serde_json::to_vec(&message).unwrap();
        //some things we do locally IF there's no connection
        match &mut shared.latest_accept {
            Some(accept) => {
                //handle proxy_only_tools
                match message.method.as_str() {
                    "tools/call" => {
                        //try proxy_only tools first
                        let tool_call_params: ToolCallParams =
                            serde_json::from_value(message.params.as_ref().unwrap().clone())
                                .unwrap();
                        let r = crate::transit::builtin_tools::call_proxy_only_tool(
                            tool_call_params.clone(),
                        );
                        match r {
                            Ok(response) => {
                                let response = Response::new(response, message.id).erase();
                                eprintln!(
                                    "transit: Sending response to proxy-only tool call: {:?}",
                                    response
                                );
                                accept
                                    .bidirectional
                                    .send(&serde_json::to_vec(&response).unwrap())?;
                                return Ok(response);
                            }
                            Err(_) => {
                                //fallthrough to remote call
                            }
                        }
                        //check specific tools
                        match tool_call_params.name.as_str() {
                            "run_latest_tool" => {
                                //here we need to get the inner tool params
                                let tool_name = tool_call_params
                                    .arguments
                                    .get("tool_name")
                                    .unwrap()
                                    .as_str()
                                    .unwrap()
                                    .to_string();
                                let tool_arguments = tool_call_params
                                    .arguments
                                    .get("params")
                                    .and_then(|v| v.as_object())
                                    .cloned()
                                    .unwrap_or_default();
                                //convert to hashmap
                                let tool_arguments: HashMap<String, serde_json::Value> =
                                    tool_arguments.into_iter().map(|(k, v)| (k, v)).collect();
                                let inner_tool_call_params =
                                    ToolCallParams::new(tool_name, tool_arguments);

                                let proxy_result =
                                    crate::transit::builtin_tools::call_proxy_only_tool(
                                        inner_tool_call_params,
                                    );
                                eprintln!(
                                    "transit: proxy_result for run_latest_tool: {:?}",
                                    proxy_result
                                );
                                match proxy_result {
                                    Ok(response) => {
                                        let response = Response::new(response, message.id).erase();
                                        return Ok(response);
                                    }
                                    Err(e) => {
                                        eprintln!("transit: Failed to call proxy-only tool: {}", e);
                                        //fallthrough to remote call
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {
                        //fallthrough to remote call
                    }
                }
                accept.bidirectional.send(&request)?;
                eprintln!(
                    "transit: Request sent to remote accept: {:?} {:?}",
                    accept.addr,
                    String::from_utf8_lossy(&request)
                );
                drop(shared);
                eprintln!("transit: Waiting for response to request: {:?}", message);
                let mut msg = self.message_receiver.recv().unwrap();
                assert!(
                    msg.id == message.id,
                    "Received response with mismatched ID: expected {:?}, got {:?}",
                    message.id,
                    msg.id
                );
                eprintln!("transit: Received response: {:?}", msg);
                //some tools we merge local and remote behaviors
                match message.method.as_str() {
                    "tools/list" => {
                        //we want to merge this with the builtin_only tools
                        let mut additional_tools =
                            crate::transit::builtin_tools::proxy_only_tools();
                        //parse tool list
                        let mut target_tool_list: ToolList =
                            serde_json::from_value(msg.result.unwrap()).unwrap();
                        target_tool_list.tools.append(&mut additional_tools.tools);
                        msg.result = Some(serde_json::to_value(target_tool_list).unwrap());
                        eprintln!("transit injected proxy-only tools into response: {:?}", msg);
                    }
                    "tools/call" => {
                        let params = message.params.as_ref().unwrap();
                        let tool_call_params: ToolCallParams =
                            serde_json::from_value(params.clone()).unwrap();
                        match tool_call_params.name.as_str() {
                            "latest_tools" => {
                                //we want to merge this with the builtin_only tools
                                let mut additional_tools =
                                    crate::transit::builtin_tools::proxy_only_tools();
                                //parse tool list
                                eprintln!("msg result before: {:?}", msg.result);
                                let mut target_response: ToolCallResponse =
                                    serde_json::from_value(msg.result.unwrap()).unwrap();
                                assert_eq!(
                                    target_response.content.len(),
                                    1,
                                    "Expected exactly one tool in response, got: {:?}",
                                    target_response.content
                                );
                                let tool_info = target_response.content.remove(0);

                                let mut target_tool_list: ToolList =
                                    serde_json::from_str(tool_info.as_str().unwrap()).unwrap();
                                target_tool_list.tools.append(&mut additional_tools.tools);
                                let as_json = serde_json::to_string(&target_tool_list).unwrap();
                                let tool_call_response =
                                    ToolCallResponse::new(vec![as_json.into()]);
                                msg.result =
                                    Some(serde_json::to_value(tool_call_response).unwrap());
                                eprintln!(
                                    "transit injected proxy-only tools into response: {:?}",
                                    msg
                                );
                            }
                            _ => {
                                //we don't do anything special for other tools
                            }
                        }
                    }
                    _ => {}
                }

                Ok(msg)
            }
            None => return Self::local_fallback(message),
        }
    }

    fn initial_setup(&mut self) {}

    fn local_fallback(
        message: crate::jrpc::Request,
    ) -> Result<crate::jrpc::Response<serde_json::Value>, Error> {
        eprintln!("transit: local fallback for request: {:?}", &message);
        match message.method.as_str() {
            "tools/list" => {
                let result = crate::transit::builtin_tools::proxy_tools();
                Ok(Response::new(result, message.id).erase())
            }
            "tools/call" => {
                let params: crate::tools::ToolCallParams =
                    serde_json::from_value(message.params.unwrap()).unwrap();
                let result = crate::transit::builtin_tools::call_proxy_tool(params);
                match result {
                    Ok(response) => Ok(Response::new(response, message.id).erase()),
                    Err(e) => Err(e.into()),
                }
            }
            _ => {
                eprintln!(
                    "transit: No connection available, cannot send request: {:?}",
                    message
                );
                return Err(Error::NotConnected);
            }
        }
    }

    /// Sends a JSON-RPC notification to the target application.
    ///
    /// # Arguments
    ///
    /// * `message` - The notification to send
    ///
    /// # Note
    ///
    /// This method is not yet implemented.
    pub fn send_notification(&mut self, _message: crate::jrpc::Notification) {
        todo!();
    }
}

impl SharedAccept {
    fn received_notification(&mut self, notification: crate::jrpc::Notification) {
        //some notifications we process locally
        match notification.method.as_str() {
            "exfiltrate/logwise/new" => {
                LogProxy::current().reset();
            }
            "exfiltrate/logwise/record" => {
                LogProxy::current().add_log(notification.params.unwrap().to_string())
            }
            _ => {
                (self.process_notifications)(notification);
            }
        }
    }

    fn new() -> Self {
        SharedAccept {
            latest_accept: None,
            process_notifications: Box::new(|_notification| {
                panic!("Notification arrived to unbound accept")
            }),
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
