use std::collections::HashMap;
use crate::jrpc::{Request, Response};
pub mod tools;
mod latest_tools;

pub fn dispatch(request: Request) -> Response<serde_json::Value> {
    if request.method == "initialize" {
        initialize(request).erase()
    } else if request.method == "tools/list" {
        tools::list(request).erase()
    }
    else if request.method == "tools/call" {
        tools::call(request).erase()
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