use std::collections::HashMap;
use std::fmt;
use std::sync::{LazyLock, RwLock};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de::{MapAccess, Visitor};
use crate::internal_proxy::InternalProxy;
use crate::jrpc::{Request, Error, Response, Notification};

pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> InputSchema;

    fn call(&self, params: HashMap<String, serde_json::Value>) -> Result<ToolCallResponse, ToolCallError>;

}




/**
These tools are avilable in the target application.
Accessing these tools from the proxy application makes little sense.
*/
pub static TOOLS: LazyLock<RwLock<Vec<Box<dyn Tool>>>> = LazyLock::new(|| {
    RwLock::new(vec![

    ])
});

pub static SHARED_TOOLS: LazyLock<Vec<Box<dyn Tool>>> = LazyLock::new(|| {
    vec![
        Box::new(crate::mcp::latest_tools::LatestTools),
        Box::new(crate::mcp::latest_tools::RunLatestTool),
    ]
});

#[derive(Debug, serde::Serialize,serde::Deserialize)]
pub struct ToolList {
    pub(crate) tools: Vec<ToolInfo>,
}

impl ToolList {
    pub fn empty() -> Self {
        ToolList { tools: Vec::new() }
    }
}

#[derive(Debug, serde::Serialize,serde::Deserialize)]
pub(crate) struct ToolInfo {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: InputSchema,
}

impl ToolInfo {
    pub(crate) fn from_tool(tool: &dyn Tool) -> Self {
        ToolInfo {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            input_schema: tool.input_schema(),
        }
    }
}

#[derive(Debug, serde::Serialize,serde::Deserialize)]
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
    let tool_infos: Vec<ToolInfo> = TOOLS.read().unwrap().iter().chain(SHARED_TOOLS.iter()).map(|tool| ToolInfo::from_tool(tool.as_ref())).collect();
    let tool_list = ToolList {
        tools: tool_infos,
    };
    tool_list
}


/**
Returns a list of tools available in the current application (e.g. the target application)
*/
pub(crate) fn list_process(request: Request) -> Response<ToolList> {
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

#[derive(Debug, serde::Deserialize,Clone)]
pub(crate) struct ToolCallParams {
    pub(crate) name: String,
    pub(crate) arguments: HashMap<String, serde_json::Value>,
}

impl ToolCallParams {
    pub(crate) fn new(name: String, arguments: HashMap<String, serde_json::Value>) -> Self {
        ToolCallParams {
            name, arguments
        }
    }
}

#[derive(Debug, serde::Serialize,serde::Deserialize)]
pub struct ToolCallResponse {
    pub(crate) content: Vec<ToolContent>,
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
impl ToolContent {
    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            ToolContent::Text(text) => Some(text),
        }
    }
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

impl<'de> Deserialize<'de> for ToolContent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de;
        enum Field { Type, Text, Unknown }

        struct ToolContentVisitor;

        impl<'de> Visitor<'de> for ToolContentVisitor {
            type Value = ToolContent;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a tool content object with type and data")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut content_type: Option<String> = None;
                let mut text: Option<String> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "type" => {
                            if content_type.is_some() {
                                return Err(de::Error::duplicate_field("type"));
                            }
                            content_type = Some(map.next_value()?);
                        }
                        "text" => {
                            if text.is_some() {
                                return Err(de::Error::duplicate_field("text"));
                            }
                            text = Some(map.next_value()?);
                        }
                        _ => {
                            let _: de::IgnoredAny = map.next_value()?;
                        }
                    }
                }

                match content_type.as_deref() {
                    Some("text") => {
                        let text = text.ok_or_else(|| de::Error::missing_field("text"))?;
                        Ok(ToolContent::Text(text))
                    }
                    Some(other) => Err(de::Error::unknown_variant(other, &["text"])),
                    None => Err(de::Error::missing_field("type")),
                }
            }
        }

        deserializer.deserialize_map(ToolContentVisitor)
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
    let tools = TOOLS.read().unwrap();
    let tool = tools.iter().chain(SHARED_TOOLS.iter())
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
            Err(Error::unknown_tool(params.name))
        }
    }
}

pub(crate) fn call(request: Request) -> Response<ToolCallResponse> {
    let params = match request.params {
        Some(params) => match serde_json::from_value::<ToolCallParams>(params) {
            Ok(params) => params,
            Err(err) => return Response::err(Error::invalid_params(err.to_string()), request.id),
        },
        None => return Response::err(Error::invalid_params("No parameters provided".to_string()), request.id),
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
