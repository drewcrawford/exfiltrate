//! Model Context Protocol (MCP) implementation.
//!
//! This module provides the core implementation of the Model Context Protocol, enabling
//! AI models and agents to interact with tools and services through a standardized
//! JSON-RPC interface. The MCP allows for dynamic tool discovery and invocation,
//! supporting both built-in and dynamically registered tools.
//!
//! # Architecture
//!
//! The MCP implementation is divided into three main components:
//!
//! - **Request Dispatch**: The [`dispatch_in_target`] function handles incoming JSON-RPC
//!   requests and routes them to appropriate handlers
//! - **Tool Management**: The [`tools`] module provides tool registration, discovery,
//!   and invocation capabilities
//! - **Dynamic Tools**: Internal tools provide runtime tool discovery, allowing agents
//!   to discover tools added after initialization
//!
//! # Usage
//!
//! The MCP system supports two primary operations:
//!
//! 1. **Tool Discovery** (`tools/list`): Returns a list of available tools with their
//!    schemas and descriptions
//! 2. **Tool Invocation** (`tools/call`): Executes a specific tool with provided parameters
//!
//! # Dynamic Tool Discovery
//!
//! The MCP includes special built-in tools for dynamic discovery:
//! - `latest_tools`: Lists all currently available tools, including those added at runtime
//! - `run_latest_tool`: Executes tools discovered through `latest_tools`
//!
//! This allows agents that cache tool lists at startup to discover and use tools that
//! were registered after initialization.
//!
//! # Examples
//!
//! ## Dispatching MCP requests
//!
//! ```
//! use exfiltrate::jrpc::{Request, Response};
//! use exfiltrate::mcp;
//! use serde_json::json;
//!
//! // Create a request to list available tools
//! let request = Request {
//!     jsonrpc: "2.0".to_string(),
//!     method: "tools/list".to_string(),
//!     params: None,
//!     id: json!(1),
//! };
//!
//! // Dispatch the request in the target application
//! let response = mcp::dispatch_in_target(request);
//! assert!(response.error.is_none() || response.result.is_some());
//! ```
//!
//! ## Registering custom tools
//!
//! ```
//! use exfiltrate::tools::{Tool, InputSchema, Argument, ToolCallResponse, ToolCallError};
//! use std::collections::HashMap;
//!
//! struct MyCustomTool;
//!
//! impl Tool for MyCustomTool {
//!     fn name(&self) -> &str {
//!         "my_tool"
//!     }
//!
//!     fn description(&self) -> &str {
//!         "A custom tool for demonstration"
//!     }
//!
//!     fn input_schema(&self) -> InputSchema {
//!         InputSchema::new(vec![
//!             Argument::new(
//!                 "input".to_string(),
//!                 "string".to_string(),
//!                 "Input text to process".to_string(),
//!                 true
//!             ),
//!         ])
//!     }
//!
//!     fn call(&self, params: HashMap<String, serde_json::Value>) 
//!         -> Result<ToolCallResponse, ToolCallError> {
//!         // Tool implementation
//!         Ok(ToolCallResponse::new(vec!["Result".into()]))
//!     }
//! }
//!
//! // Register the tool
//! exfiltrate::tools::add_tool(Box::new(MyCustomTool));
//! 
//! // Verify it was added
//! let request = exfiltrate::jrpc::Request {
//!     jsonrpc: "2.0".to_string(),
//!     method: "tools/list".to_string(),
//!     params: None,
//!     id: serde_json::json!(1),
//! };
//! let response = exfiltrate::mcp::dispatch_in_target(request);
//! // The tool list will include our custom tool
//! assert!(response.result.is_some());
//! ```

use crate::jrpc::{Request, Response};
pub mod tools;
pub(crate) mod latest_tools;

/// Dispatches incoming JSON-RPC requests to appropriate handlers in the target application.
///
/// This is the main entry point for handling MCP protocol requests. It routes requests
/// based on their method name to the appropriate handler functions.
///
/// # Supported Methods
///
/// - `tools/list`: Returns a list of all available tools with their schemas
/// - `tools/call`: Invokes a specific tool with provided parameters
///
/// # Arguments
///
/// * `request` - The JSON-RPC request to process
///
/// # Returns
///
/// A JSON-RPC response containing either the result of the operation or an error
///
/// # Examples
///
/// ```
/// use exfiltrate::jrpc::Request;
/// use exfiltrate::mcp;
/// use serde_json::json;
///
/// let request = Request {
///     jsonrpc: "2.0".to_string(),
///     method: "tools/list".to_string(),
///     params: None,
///     id: json!("req-1"),
/// };
///
/// let response = mcp::dispatch_in_target(request);
/// // Either we get a result or an error, not both
/// assert!(response.result.is_some() != response.error.is_some());
/// ```
pub fn dispatch_in_target(request: Request) -> Response<serde_json::Value> {
    if request.method == "tools/list" {
        tools::list_process(request).erase()
    }
    else if request.method == "tools/call" {
        tools::call(request).erase()
    }
    else {
        Response::err(super::jrpc::Error::method_not_found(), request.id)
    }
}

