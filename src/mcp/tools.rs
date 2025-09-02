//! Tool management and invocation system for the Model Context Protocol.
//!
//! This module provides the infrastructure for registering, discovering, and invoking
//! tools within the MCP framework. Tools are functions that can be called remotely
//! through the JSON-RPC protocol, allowing AI agents to interact with external services
//! and perform actions.
//!
//! # Architecture
//!
//! The module manages two collections of tools:
//!
//! - **Target Tools** (`TOOLS`): Tools available only in the target application
//! - **Shared Tools** (`SHARED_TOOLS`): Tools available in both proxy and target applications
//!
//! # Tool Implementation
//!
//! Tools must implement the [`Tool`] trait, which defines:
//! - Metadata (name and description)
//! - Input schema for parameter validation
//! - Execution logic
//!
//! # Examples
//!
//! ## Implementing a custom tool
//!
//! ```
//! use exfiltrate::mcp::tools::{Tool, InputSchema, Argument, ToolCallResponse, ToolCallError};
//! use std::collections::HashMap;
//!
//! struct CalculatorTool;
//!
//! impl Tool for CalculatorTool {
//!     fn name(&self) -> &str {
//!         "calculator"
//!     }
//!
//!     fn description(&self) -> &str {
//!         "Performs basic arithmetic operations"
//!     }
//!
//!     fn input_schema(&self) -> InputSchema {
//!         InputSchema::new(vec![
//!             Argument::new(
//!                 "operation".to_string(),
//!                 "string".to_string(),
//!                 "Operation to perform (add, subtract, multiply, divide)".to_string(),
//!                 true
//!             ),
//!             Argument::new(
//!                 "a".to_string(),
//!                 "number".to_string(),
//!                 "First operand".to_string(),
//!                 true
//!             ),
//!             Argument::new(
//!                 "b".to_string(),
//!                 "number".to_string(),
//!                 "Second operand".to_string(),
//!                 true
//!             ),
//!         ])
//!     }
//!
//!     fn call(&self, params: HashMap<String, serde_json::Value>)
//!         -> Result<ToolCallResponse, ToolCallError> {
//!         let op = params.get("operation")
//!             .and_then(|v| v.as_str())
//!             .ok_or_else(|| ToolCallError::new(vec!["Missing operation".into()]))?;
//!         
//!         let a = params.get("a")
//!             .and_then(|v| v.as_f64())
//!             .ok_or_else(|| ToolCallError::new(vec!["Invalid operand a".into()]))?;
//!         
//!         let b = params.get("b")
//!             .and_then(|v| v.as_f64())
//!             .ok_or_else(|| ToolCallError::new(vec!["Invalid operand b".into()]))?;
//!         
//!         let result = match op {
//!             "add" => a + b,
//!             "subtract" => a - b,
//!             "multiply" => a * b,
//!             "divide" => {
//!                 if b == 0.0 {
//!                     return Err(ToolCallError::new(vec!["Division by zero".into()]));
//!                 }
//!                 a / b
//!             },
//!             _ => return Err(ToolCallError::new(vec!["Unknown operation".into()]))
//!         };
//!         
//!         Ok(ToolCallResponse::new(vec![format!("Result: {}", result).into()]))
//!     }
//! }
//!
//! // Register and use the tool
//! exfiltrate::mcp::tools::add_tool(Box::new(CalculatorTool));
//! ```

use crate::internal_proxy::InternalProxy;
use crate::jrpc::{Error, Notification, Request, Response};
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::fmt;
use std::sync::{LazyLock, RwLock};

/// Trait for implementing MCP tools.
///
/// Tools are functions that can be invoked remotely through the MCP protocol.
/// Each tool must provide metadata about itself and implement the execution logic.
///
/// # Thread Safety
///
/// Tools must be `Send + Sync` as they may be called from multiple threads.
///
/// # Examples
///
/// ```
/// use exfiltrate::mcp::tools::{Tool, InputSchema, Argument, ToolCallResponse, ToolCallError};
/// use std::collections::HashMap;
///
/// struct EchoTool;
///
/// impl Tool for EchoTool {
///     fn name(&self) -> &str {
///         "echo"
///     }
///
///     fn description(&self) -> &str {
///         "Echoes back the input message"
///     }
///
///     fn input_schema(&self) -> InputSchema {
///         InputSchema::new(vec![
///             Argument::new(
///                 "message".to_string(),
///                 "string".to_string(),
///                 "Message to echo".to_string(),
///                 true
///             ),
///         ])
///     }
///
///     fn call(&self, params: HashMap<String, serde_json::Value>)
///         -> Result<ToolCallResponse, ToolCallError> {
///         let message = params.get("message")
///             .and_then(|v| v.as_str())
///             .ok_or_else(|| ToolCallError::new(vec!["Missing message".into()]))?;
///         
///         Ok(ToolCallResponse::new(vec![format!("Echo: {}", message).into()]))
///     }
/// }
/// ```
pub trait Tool: Send + Sync {
    /// Returns the unique name of the tool.
    ///
    /// This name is used to identify the tool in MCP requests.
    fn name(&self) -> &str;

    /// Returns a human-readable description of what the tool does.
    ///
    /// This description is shown to users and AI agents to help them
    /// understand the tool's purpose.
    fn description(&self) -> &str;

    /// Returns the schema defining the tool's input parameters.
    ///
    /// The schema specifies what parameters the tool accepts, their types,
    /// and whether they are required.
    fn input_schema(&self) -> InputSchema;

    /// Executes the tool with the provided parameters.
    ///
    /// # Arguments
    ///
    /// * `params` - A map of parameter names to their JSON values
    ///
    /// # Returns
    ///
    /// * `Ok(ToolCallResponse)` - Success response with tool output
    /// * `Err(ToolCallError)` - Error response if the tool execution fails
    fn call(
        &self,
        params: HashMap<String, serde_json::Value>,
    ) -> Result<ToolCallResponse, ToolCallError>;
}

/// Tools available in the target application.
///
/// This collection stores tools that are specific to the target application
/// and should not be accessed directly from the proxy. Tools can be dynamically
/// added at runtime using [`add_tool`].
///
/// # Thread Safety
///
/// The collection is protected by a `RwLock` to allow concurrent reads and
/// exclusive writes.
pub(crate) static TOOLS: LazyLock<RwLock<Vec<Box<dyn Tool>>>> = LazyLock::new(|| RwLock::new(vec![]));

/// Tools available in both proxy and target applications.
///
/// These tools provide core functionality that is useful in both contexts,
/// such as dynamic tool discovery.
pub(crate) static SHARED_TOOLS: LazyLock<Vec<Box<dyn Tool>>> = LazyLock::new(|| {
    vec![
        Box::new(crate::mcp::latest_tools::LatestTools),
        Box::new(crate::mcp::latest_tools::RunLatestTool),
    ]
});

/// A collection of tool information.
///
/// Used to return tool metadata in response to `tools/list` requests.
///
/// # Examples
///
/// ```
/// use exfiltrate::mcp::tools::ToolList;
///
/// let empty_list = ToolList::empty();
/// // The list is created empty
/// let json = serde_json::to_string(&empty_list).unwrap();
/// assert!(json.contains("\"tools\":[]"));
/// ```
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ToolList {
    /// The list of available tools with their metadata
    pub(crate) tools: Vec<ToolInfo>,
}

impl ToolList {
    /// Creates an empty tool list.
    ///
    /// # Examples
    ///
    /// ```
    /// use exfiltrate::mcp::tools::ToolList;
    ///
    /// let list = ToolList::empty();
    /// // Verify it serializes as an empty list
    /// let json = serde_json::to_string(&list).unwrap();
    /// assert!(json.contains("\"tools\":[]"));
    /// ```
    pub fn empty() -> Self {
        ToolList { tools: Vec::new() }
    }
}

/// Metadata about a tool.
///
/// Contains all the information needed for an agent to understand
/// and invoke a tool.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct ToolInfo {
    /// The unique name of the tool
    name: String,
    /// Human-readable description of the tool's purpose
    description: String,
    /// Schema defining the tool's input parameters
    #[serde(rename = "inputSchema")]
    input_schema: InputSchema,
}

impl ToolInfo {
    /// Creates tool info from a Tool trait object.
    ///
    /// Extracts metadata from the tool implementation.
    pub(crate) fn from_tool(tool: &dyn Tool) -> Self {
        ToolInfo {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            input_schema: tool.input_schema(),
        }
    }
}

/// Schema defining a tool's input parameters.
///
/// Follows JSON Schema format to describe the structure and validation
/// rules for tool parameters.
///
/// # Examples
///
/// ```
/// use exfiltrate::mcp::tools::{InputSchema, Argument};
///
/// let schema = InputSchema::new(vec![
///     Argument::new(
///         "text".to_string(),
///         "string".to_string(),
///         "Input text".to_string(),
///         true
///     ),
///     Argument::new(
///         "count".to_string(),
///         "number".to_string(),
///         "Optional count".to_string(),
///         false
///     ),
/// ]);
/// ```
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct InputSchema {
    /// The schema type (always "object" for tool parameters)
    r#type: String,
    /// Map of parameter names to their schema definitions
    properties: HashMap<String, HashMap<String, serde_json::Value>>,
    /// List of required parameter names
    required: Vec<String>,
}

/// Represents a single parameter for a tool.
///
/// Used to construct input schemas for tools.
///
/// # Examples
///
/// ```
/// use exfiltrate::mcp::tools::Argument;
///
/// let arg = Argument::new(
///     "filename".to_string(),
///     "string".to_string(),
///     "Path to the file".to_string(),
///     true  // required
/// );
/// ```
pub struct Argument {
    /// The parameter name
    name: String,
    /// The parameter type (e.g., "string", "number", "boolean", "object")
    r#type: String,
    /// Human-readable description of the parameter
    description: String,
    /// Whether this parameter is required
    required: bool,
}

impl Argument {
    /// Creates a new tool argument specification.
    ///
    /// # Arguments
    ///
    /// * `name` - The parameter name
    /// * `type` - The JSON type ("string", "number", "boolean", "object", "array")
    /// * `description` - Human-readable description
    /// * `required` - Whether the parameter is required
    ///
    /// # Examples
    ///
    /// ```
    /// use exfiltrate::mcp::tools::Argument;
    ///
    /// let required_arg = Argument::new(
    ///     "input".to_string(),
    ///     "string".to_string(),
    ///     "The input text to process".to_string(),
    ///     true
    /// );
    ///
    /// let optional_arg = Argument::new(
    ///     "verbose".to_string(),
    ///     "boolean".to_string(),
    ///     "Enable verbose output".to_string(),
    ///     false
    /// );
    /// ```
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
    /// Creates a new input schema from a collection of arguments.
    ///
    /// Converts the argument specifications into a JSON Schema format
    /// suitable for parameter validation.
    ///
    /// # Arguments
    ///
    /// * `arguments` - An iterator of [`Argument`] specifications
    ///
    /// # Examples
    ///
    /// ```
    /// use exfiltrate::mcp::tools::{InputSchema, Argument};
    ///
    /// let schema = InputSchema::new(vec![
    ///     Argument::new(
    ///         "query".to_string(),
    ///         "string".to_string(),
    ///         "Search query".to_string(),
    ///         true
    ///     ),
    ///     Argument::new(
    ///         "limit".to_string(),
    ///         "number".to_string(),
    ///         "Maximum results".to_string(),
    ///         false
    ///     ),
    /// ]);
    /// ```
    pub fn new<A: IntoIterator<Item = Argument>>(arguments: A) -> Self {
        let mut properties = HashMap::new();
        let mut required = Vec::new();
        for argument in arguments {
            let mut inner_map: HashMap<String, serde_json::Value> = HashMap::new();
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

/// Internal function to list all available tools.
///
/// Combines tools from both [`TOOLS`] and [`SHARED_TOOLS`] collections.
pub(crate) fn list_int() -> ToolList {
    let tool_infos: Vec<ToolInfo> = TOOLS
        .read()
        .unwrap()
        .iter()
        .chain(SHARED_TOOLS.iter())
        .map(|tool| ToolInfo::from_tool(tool.as_ref()))
        .collect();
    let tool_list = ToolList { tools: tool_infos };
    tool_list
}

/// Processes a `tools/list` request.
///
/// Returns a list of all tools available in the current application
/// (target application context).
///
/// # Arguments
///
/// * `request` - The JSON-RPC request
///
/// # Returns
///
/// A response containing the list of available tools
pub(crate) fn list_process(request: Request) -> Response<ToolList> {
    let tool_list = list_int();
    let response = Response::new(tool_list, request.id);
    response
}

/// Registers a new tool in the target application.
///
/// Adds the tool to the `TOOLS` collection and sends a notification
/// to inform connected clients that the tool list has changed.
///
/// # Arguments
///
/// * `tool` - The tool implementation to register
///
/// # Examples
///
/// ```
/// use exfiltrate::mcp::tools::{Tool, InputSchema, ToolCallResponse, ToolCallError, add_tool};
/// use std::collections::HashMap;
///
/// struct MyTool;
///
/// impl Tool for MyTool {
///     fn name(&self) -> &str { "my_tool" }
///     fn description(&self) -> &str { "A custom tool" }
///     fn input_schema(&self) -> InputSchema { InputSchema::new(vec![]) }
///     fn call(&self, _: HashMap<String, serde_json::Value>)
///         -> Result<ToolCallResponse, ToolCallError> {
///         Ok(ToolCallResponse::new(vec!["Success".into()]))
///     }
/// }
///
/// add_tool(Box::new(MyTool));
/// ```
pub fn add_tool(tool: Box<dyn Tool>) {
    TOOLS.write().unwrap().push(tool);
    //create a tool changed message
    let n = Notification::new("notifications/tools/list_changed".to_string(), None);
    let r = InternalProxy::current().send_notification(n);
    match r {
        Ok(_) => {}
        Err(crate::internal_proxy::Error::NotConnected) => {
            //benign
        }
    }
}

/// Parameters for invoking a tool.
///
/// Used internally to deserialize tool call requests.
#[derive(Debug, serde::Deserialize, Clone)]
pub(crate) struct ToolCallParams {
    /// Name of the tool to invoke
    pub(crate) name: String,
    /// Arguments to pass to the tool
    pub(crate) arguments: HashMap<String, serde_json::Value>,
}

impl ToolCallParams {
    /// Creates new tool call parameters.
    pub(crate) fn new(name: String, arguments: HashMap<String, serde_json::Value>) -> Self {
        ToolCallParams { name, arguments }
    }
}

/// Response from a successful tool invocation.
///
/// Contains the output content from the tool execution.
///
/// # Examples
///
/// ```
/// use exfiltrate::mcp::tools::ToolCallResponse;
///
/// let response = ToolCallResponse::new(vec![
///     "Operation completed successfully".into(),
///     "Result: 42".into(),
/// ]);
/// ```
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ToolCallResponse {
    /// The content returned by the tool
    pub(crate) content: Vec<ToolContent>,
    /// Whether this response represents an error
    is_error: bool,
}

impl ToolCallResponse {
    /// Creates a new successful tool response.
    ///
    /// # Arguments
    ///
    /// * `content` - The content to return from the tool
    ///
    /// # Examples
    ///
    /// ```
    /// use exfiltrate::mcp::tools::ToolCallResponse;
    ///
    /// let response = ToolCallResponse::new(vec![
    ///     "Task completed".into(),
    /// ]);
    /// ```
    pub fn new(content: Vec<ToolContent>) -> Self {
        ToolCallResponse {
            content,
            is_error: false,
        }
    }
}

/// Error response from a failed tool invocation.
///
/// Contains error messages explaining why the tool execution failed.
///
/// # Examples
///
/// ```
/// use exfiltrate::mcp::tools::ToolCallError;
///
/// let error = ToolCallError::new(vec![
///     "Invalid input: missing required parameter 'filename'".into(),
/// ]);
/// ```
#[derive(Debug, serde::Serialize)]
pub struct ToolCallError {
    /// Error messages
    content: Vec<ToolContent>,
    /// Always true for error responses
    is_error: bool,
}

impl ToolCallError {
    /// Creates a new tool error response.
    ///
    /// # Arguments
    ///
    /// * `content` - Error messages to return
    ///
    /// # Examples
    ///
    /// ```
    /// use exfiltrate::mcp::tools::ToolCallError;
    ///
    /// let error = ToolCallError::new(vec![
    ///     "Database connection failed".into(),
    ///     "Please check your connection settings".into(),
    /// ]);
    /// ```
    pub fn new(content: Vec<ToolContent>) -> Self {
        ToolCallError {
            content,
            is_error: true,
        }
    }

    /// Converts this error into a ToolCallResponse.
    ///
    /// Used internally to unify error and success responses.
    pub(crate) fn into_response(self) -> ToolCallResponse {
        ToolCallResponse {
            content: self.content,
            is_error: true,
        }
    }
}

/// Content returned by a tool.
///
/// Currently supports text content, but marked as `non_exhaustive`
/// to allow for future content types (e.g., images, structured data).
///
/// # Examples
///
/// ```
/// use exfiltrate::mcp::tools::ToolContent;
///
/// let text_content = ToolContent::from("Hello, world!");
/// let string_content = ToolContent::from(String::from("Dynamic content"));
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub enum ToolContent {
    /// Text content
    Text(String),
}
impl ToolContent {
    /// Returns the content as a string slice if it's text content.
    ///
    /// # Returns
    ///
    /// * `Some(&str)` if the content is text
    /// * `None` for other content types (when added in the future)
    #[cfg(feature="transit")]
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

/// Internal implementation for calling a tool.
///
/// Looks up the tool by name and invokes it with the provided arguments.
/// Searches both [`TOOLS`] and [`SHARED_TOOLS`] collections.
pub(crate) fn call_imp(params: ToolCallParams) -> Result<ToolCallResponse, crate::jrpc::Error> {
    let tools = TOOLS.read().unwrap();
    let tool = tools
        .iter()
        .chain(SHARED_TOOLS.iter())
        .find(|t| t.name() == params.name)
        .map(|t| t.as_ref());
    match tool {
        Some(tool) => {
            let call = tool.call(params.arguments);
            match call {
                Ok(response) => Ok(response),
                Err(err) => Ok(err.into_response()),
            }
        }
        None => Err(Error::unknown_tool(params.name)),
    }
}

/// Processes a `tools/call` request.
///
/// Parses the request parameters and invokes the specified tool.
///
/// # Arguments
///
/// * `request` - The JSON-RPC request containing tool name and arguments
///
/// # Returns
///
/// A response containing either the tool's output or an error
pub(crate) fn call(request: Request) -> Response<ToolCallResponse> {
    let params = match request.params {
        Some(params) => match serde_json::from_value::<ToolCallParams>(params) {
            Ok(params) => params,
            Err(err) => return Response::err(Error::invalid_params(err.to_string()), request.id),
        },
        None => {
            return Response::err(
                Error::invalid_params("No parameters provided".to_string()),
                request.id,
            );
        }
    };
    let r = call_imp(params);
    match r {
        Ok(r) => Response::new(r, request.id),
        Err(e) => Response::err(e, request.id),
    }
}
