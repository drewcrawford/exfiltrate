use std::collections::HashMap;
use crate::core::jrpc::{Request, Response};
pub mod logging;

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
    capabilities: HashMap<String, HashMap<String, serde_json::Value>>,
    #[serde(rename = "serverInfo")]
    server_info: HashMap<String, serde_json::Value>,

}

impl InitializeResult {
    fn new() -> Self {
        let mut server_info = HashMap::new();
        server_info.insert("name".to_string(), "exfiltrate".into());
        server_info.insert("version".to_string(), "1.0.0".into());

        let mut capabilities = HashMap::new();
        capabilities.insert("logging".to_string(), HashMap::new());
        capabilities.insert("tools".to_string(), HashMap::new());
        InitializeResult {
            protocol_version: "2025-03-26".to_string(),
            capabilities,
            server_info,

        }
    }
}