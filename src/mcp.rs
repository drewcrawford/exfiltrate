//! Model Context Protocol (MCP) implementation.
//!
//! This module provides the core implementation of the Model Context Protocol, enabling
//! AI models and agents to interact with tools and services through a standardized
//! JSON-RPC interface. The MCP allows for dynamic tool discovery and invocation,
//! supporting both built-in and dynamically registered tools.
//!
//! # Architecture
//!
//! The MCP implementation is divided into main components:
//!
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
//! ## Registering custom tools
//!
//! ```
//! use exfiltrate::mcp::tools::{Tool, InputSchema, Argument, ToolCallResponse, ToolCallError};
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
//! exfiltrate::mcp::tools::add_tool(Box::new(MyCustomTool));
//!
//! ```

use crate::jrpc::{Request, Response};
pub(crate) mod latest_tools;
pub mod tools;

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
pub(crate) fn dispatch_in_target(request: Request) -> Response<serde_json::Value> {
    if request.method == "tools/list" {
        tools::list_process(request).erase()
    } else if request.method == "tools/call" {
        tools::call(request).erase()
    } else {
        Response::err(super::jrpc::Error::method_not_found(), request.id)
    }
}
