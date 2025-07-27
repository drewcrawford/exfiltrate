use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use serde::{Serialize, Serializer};
use crate::core::jrpc::{Request, Response};
use crate::core::mcp::InitializeResult;

pub trait Tool {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> InputSchema;

    fn call(&self, params: HashMap<String, serde_json::Value>) -> Result<ToolCallResponse, ToolCallError>;
}

pub const TOOLS: LazyLock<Mutex<Vec<Box<dyn Tool>>>> = LazyLock::new(|| {
    Mutex::new(vec![
    ])
});

#[derive(Debug, serde::Serialize)]
pub struct ToolList {
    tools: Vec<ToolInfo>,
}

#[derive(Debug, serde::Serialize)]
struct ToolInfo {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: InputSchema,
}

impl ToolInfo {
    fn from_tool(tool: &dyn Tool) -> Self {
        ToolInfo {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            input_schema: tool.input_schema(),
        }
    }
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

impl ToolInfo {
    fn new(name: &str, description: &str) -> Self {
        ToolInfo {
            name: name.to_string(),
            description: description.to_string(),
            input_schema: InputSchema::new(),
        }
    }
}

pub fn list(request: Request) -> Response<ToolList> {
    let tools: Vec<ToolInfo> = TOOLS.lock().unwrap().iter().map(|tool| ToolInfo::from_tool(tool.as_ref())).collect();
    let tool_list = ToolList {
        tools,
    };
    let response = Response::new(tool_list, request.id);
    response
}

pub fn add_tool(tool: Box<dyn Tool>) {
    TOOLS.lock().unwrap().push(tool);
}

#[derive(Debug, serde::Deserialize)]
struct ToolCallParams {
    name: String,
    arguments: HashMap<String, serde_json::Value>,
}

#[derive(Debug, serde::Serialize)]
pub struct ToolCallResponse {
    content: Vec<ToolContent>,
    is_error: bool,
}

impl ToolCallResponse {
    pub fn new(content: Vec<ToolContent>) -> Self {
        ToolCallResponse {
            content,
            is_error: false,
        }
    }
}


#[derive(Debug, serde::Serialize)]
pub struct ToolCallError {
    content: Vec<ToolContent>,
    is_error: bool,
}

impl ToolCallError {
    pub fn new(content: Vec<ToolContent>) -> Self {
        ToolCallError {
            content,
            is_error: true,
        }
    }
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
    //look up tool
    let binding = TOOLS;
    let tools = binding.lock().unwrap();
    let tool = tools.iter()
        .find(|t| t.name() == params.name)
        .map(|t| t.as_ref())
        .ok_or_else(|| crate::core::jrpc::Error::new(-32602, format!("Unknown tool: {}", params.name), None));
    match tool {
        Ok(tool) => {
            todo!()
        }
        Err(err) => {
            return Response::err(err, request.id);
        }
    }

}