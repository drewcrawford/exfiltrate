use std::collections::HashMap;
use crate::core::jrpc::{Request, Response};
use crate::core::mcp::InitializeResult;

#[derive(Debug, serde::Serialize)]
pub struct ToolList {
    tools: Vec<Tool>,
}

#[derive(Debug, serde::Serialize)]
struct Tool {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: InputSchema,
}

#[derive(Debug, serde::Serialize)]
struct InputSchema {
    r#type: String,
    properties: HashMap<String, HashMap<String, serde_json::Value>>,
    required: Vec<String>,
}

impl InputSchema {
    fn new() -> Self {
        InputSchema {
            r#type: "object".to_string(),
            properties: HashMap::new(),
            required: Vec::new(),
        }
    }
}

impl Tool {
    fn new(name: &str, description: &str) -> Self {
        Tool {
            name: name.to_string(),
            description: description.to_string(),
            input_schema: InputSchema::new(),
        }
    }
}

pub fn list(request: Request) -> Response<ToolList> {
    let tool_list = ToolList {
        tools: vec![
            Tool::new("analyze", "Analyze data using various algorithms"),
        ],
    };
    let response = Response::new(tool_list, request.id);
    response
}