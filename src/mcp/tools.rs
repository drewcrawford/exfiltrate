use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, RwLock};
use serde::{Serialize, Serializer};
use crate::internal_proxy::InternalProxy;
use crate::jrpc::{Request, Error, Response, Notification};

pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> InputSchema;

    fn call(&self, params: HashMap<String, serde_json::Value>) -> Result<ToolCallResponse, ToolCallError>;
}

pub static TOOLS: LazyLock<RwLock<Vec<Box<dyn Tool>>>> = LazyLock::new(|| {
    RwLock::new(vec![
        Box::new(crate::mcp::latest_tools::LatestTools),
        Box::new(crate::mcp::latest_tools::RunLatestTool),
    ])
});

#[derive(Debug, serde::Serialize)]
pub struct ToolList {
    tools: Vec<ToolInfo>,
}

impl ToolList {
    pub fn empty() -> Self {
        ToolList { tools: Vec::new() }
    }
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
pub struct InputSchema {
    r#type: String,
    properties: HashMap<String, HashMap<String, serde_json::Value>>,
    required: Vec<String>,
}

pub struct Argument {
    name: String,
    r#type: String,
    description: String,
    required: bool,
}

impl Argument {
    pub fn new(name: String, r#type: String, description: String, required: bool) -> Self {
        Self {
            name,
            r#type,
            description,
            required,
        }
    }
}

impl InputSchema {
    pub fn new<A: IntoIterator<Item=Argument>>(arguments: A) -> Self {
        let mut properties = HashMap::new();
        let mut required = Vec::new();
        for argument in arguments {
            let mut inner_map: HashMap<String,serde_json::Value> = HashMap::new();
            inner_map.insert("type".to_string(), argument.r#type.into());
            inner_map.insert("description".to_string(), argument.description.into());
            if argument.required {
                required.push(argument.name.clone());
            }
            properties.insert(argument.name, inner_map);
        }
        InputSchema {
            r#type: "object".to_string(),
            properties,
            required,
        }
    }
}

impl ToolInfo {
    fn new(name: String, description: String, input_schema: InputSchema) -> Self {
        ToolInfo {
            name,
            description,
            input_schema,
        }
    }
}

pub(crate) fn list_int() -> ToolList {
    let tools: Vec<ToolInfo> = TOOLS.read().unwrap().iter().map(|tool| ToolInfo::from_tool(tool.as_ref())).collect();
    let tool_list = ToolList {
        tools,
    };
    tool_list
}

pub(crate) fn list(request: Request) -> Response<ToolList> {
    let tool_list = list_int();
    let response = Response::new(tool_list, request.id);
    response
}

pub fn add_tool(tool: Box<dyn Tool>) {
    TOOLS.write().unwrap().push(tool);
    //create a tool changed message
    let n = Notification::new("notifications/tools/list_changed".to_string(), None);
    let r = InternalProxy::current().send_notification(n);
    match r {
        Ok(_) => {},
        Err(crate::internal_proxy::Error::NotConnected) => {
            //benign
        }
        Err(other) => {
            eprintln!("Error sending notification: {other:?}")
        }
    }

}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct ToolCallParams {
    name: String,
    arguments: HashMap<String, serde_json::Value>,
}

impl ToolCallParams {
    pub(crate) fn new(name: String, arguments: HashMap<String, serde_json::Value>) -> Self {
        ToolCallParams {
            name, arguments
        }
    }
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
    pub(crate) fn into_response(self) -> ToolCallResponse {
        ToolCallResponse {
            content: self.content,
            is_error: true,
        }
    }
}



#[derive(Debug)]
#[non_exhaustive]
pub enum ToolContent {
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

impl From<String> for ToolContent {
    fn from(value: String) -> Self {
        ToolContent::Text(value)
    }
}

impl From<&str> for ToolContent {
    fn from(value: &str) -> Self {
        ToolContent::Text(value.to_string())
    }
}

pub(crate) fn call_imp(params: ToolCallParams) -> Result<ToolCallResponse, crate::jrpc::Error>  {
    //look up tool
    let tools = TOOLS.read().unwrap();
    let tool = tools.iter()
        .find(|t| t.name() == params.name)
        .map(|t| t.as_ref());
    match tool {
        Some(tool) => {
            let call = tool.call(params.arguments);
            match call {
                Ok(response) => Ok(response),
                Err(err) => Ok(err.into_response())
            }
        }
        None => {
            Err(Error::new(-32602, format!("Unknown tool: {}", params.name), None))
        }
    }
}

pub(crate) fn call(request: Request) -> Response<ToolCallResponse> {
    let params = match request.params {
        Some(params) => match serde_json::from_value::<ToolCallParams>(params) {
            Ok(params) => params,
            Err(err) => return Response::err(Error::new(-32602, "Invalid params".to_string(), Some(err.to_string().into())), request.id),
        },
        None => return Response::err(Error::new(-32602, "Invalid params".to_string(), Some("No parameters specified".into())), request.id),
    };
    let r = call_imp(params);
    match r {
        Ok(r) => {
            Response::new(r, request.id)
        }
        Err(e) => {
            Response::err(e, request.id)
        }
    }
}