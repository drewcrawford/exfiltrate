use std::collections::HashMap;
use serde::{Serialize, Serializer};
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

#[derive(Debug, serde::Deserialize)]
struct ToolCallParams {
    name: String,
    arguments: HashMap<String, serde_json::Value>,
}

#[derive(Debug, serde::Serialize)]
pub struct ToolCallResponse {
    content: Vec<ToolContent>,
}

#[derive(Debug)]
enum ToolContent {
    Text(String),
}

impl Serialize for ToolContent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        match self {
            ToolContent::Text(text) => {
                let mut s = serializer.serialize_struct("ToolContent", 2)?;

                s.serialize_field("type", "text")?;
                s.serialize_field("text", text)?;
                s.end()
            }
        }
    }
}

pub fn call(request: Request) -> Response<ToolCallResponse> {
    let params = match request.params {
        Some(params) => match serde_json::from_value::<ToolCallParams>(params) {
            Ok(params) => params,
            Err(err) => return Response::err(crate::core::jrpc::Error::new(-32602, "Invalid params".to_string(), Some(err.to_string().into())), request.id),
        },
        None => return Response::err(crate::core::jrpc::Error::new(-32602, "Invalid params".to_string(), Some("No parameters specified".into())), request.id),
    };
    let response = Response::new(ToolCallResponse {
        content: vec![ToolContent::Text("Tool call executed successfully".to_string())],
    }, request.id);

    response
}