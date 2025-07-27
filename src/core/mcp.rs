use std::collections::HashMap;
use crate::core::jrpc::{Request, Response};

pub fn dispatch(request: Request) -> Response<serde_json::Value> {
    if request.method == "initialize" {
        initialize(request).erase()
    }
    else {
        Response::err(super::jrpc::Error::new(-32601, "Method not found".to_string(), None), request.id)
    }
}

fn initialize(request: Request) -> Response<InitializeResult> {
    Response::new(InitializeResult::new(), request.id)
}

#[derive(Debug, serde::Serialize)]
struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    protocol_version: String,
    capabilities: HashMap<String, serde_json::Value>,
    #[serde(rename = "serverInfo")]
    server_info: HashMap<String, serde_json::Value>,

}

impl InitializeResult {
    fn new() -> Self {
        let mut server_info = HashMap::new();
        server_info.insert("name".to_string(), "exfiltrate".into());
        server_info.insert("version".to_string(), "1.0.0".into());
        InitializeResult {
            protocol_version: "2025-03-26".to_string(),
            capabilities: HashMap::new(),
            server_info: server_info,

        }
    }
}