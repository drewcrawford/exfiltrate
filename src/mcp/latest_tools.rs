// SPDX-License-Identifier: MIT OR Apache-2.0
//! Dynamic tool discovery system for the Model Context Protocol.
//!
//! This module provides tools that enable runtime discovery and invocation of
//! dynamically registered tools. This is essential for AI agents that cache tool
//! lists at startup but need to access tools added during their session.
//!
//! # Purpose
//!
//! Many AI agents and LLM interfaces cache the available tool list when they start
//! a session. If new tools are registered after initialization (via [`crate::mcp::tools::add_tool`]),
//! these agents won't see them in their cached list. The tools in this module solve
//! this problem by providing:
//!
//! - [`LatestTools`]: A tool that returns the current tool list at call time
//! - [`RunLatestTool`]: A tool that can invoke any tool by name, including newly added ones
//!
//! # Usage Pattern
//!
//! 1. Agent starts with cached tool list
//! 2. New tools are added dynamically via [`crate::mcp::tools::add_tool`]
//! 3. Agent calls `latest_tools` to discover new tools
//! 4. Agent calls `run_latest_tool` to invoke the newly discovered tools
//!
//! # Examples
//!
//! ```
//! use exfiltrate::mcp::tools::{Tool, InputSchema, Argument, ToolCallResponse, ToolCallError};
//! use std::collections::HashMap;
//!
//! // Define a simple tool that can be added dynamically
//! struct DynamicTool;
//! impl Tool for DynamicTool {
//!     fn name(&self) -> &str { "dynamic_tool" }
//!     fn description(&self) -> &str { "A tool added at runtime" }
//!     fn input_schema(&self) -> InputSchema {
//!         InputSchema::new(vec![])
//!     }
//!     fn call(&self, _: HashMap<String, serde_json::Value>)
//!         -> Result<ToolCallResponse, ToolCallError> {
//!         Ok(ToolCallResponse::new(vec!["Dynamic response".into()]))
//!     }
//! }
//!
//! // Add the tool dynamically
//! exfiltrate::mcp::tools::add_tool(Box::new(DynamicTool));
//!
//! // The agent can now discover this tool using `latest_tools`
//! // and invoke it using `run_latest_tool` with name "dynamic_tool"
//! ```

use crate::mcp::tools::{InputSchema, Tool, ToolCallError, ToolCallParams, ToolCallResponse};
use serde_json::Value;
use std::collections::HashMap;

/// Tool for discovering the current list of available tools at runtime.
///
/// This tool returns the complete list of tools available at the moment it's called,
/// including any tools that were added after the agent's initial tool discovery.
/// The output is a JSON representation of all available tools with their schemas.
///
/// # Purpose
///
/// Enables agents with cached tool lists to discover newly registered tools
/// without restarting their session.
///
/// # Output Format
///
/// Returns a JSON string containing a `ToolList` structure with all available tools
/// and their metadata (name, description, and input schema).
///
pub struct LatestTools;

impl Tool for LatestTools {
    fn name(&self) -> &str {
        "latest_tools"
    }

    fn description(&self) -> &str {
        "Discovers the latest tools available at runtime.
        Some agents cache tools on startup and may not have an up-to-date list when tools
        are added or removed during a session.  This tool lists the tools that are current available
        at the time of the call, which may be more up-to-date than the cached tools.

        To run a tool discovered by this tool, use the `run_latest_tool` tool."
    }

    fn call(
        &self,
        _params: std::collections::HashMap<String, serde_json::Value>,
    ) -> Result<crate::mcp::tools::ToolCallResponse, crate::mcp::tools::ToolCallError> {
        let tools = crate::mcp::tools::list_int();
        let text = serde_json::to_string(&tools).unwrap();
        Ok(crate::mcp::tools::ToolCallResponse::new(vec![text.into()]))
    }

    fn input_schema(&self) -> crate::mcp::tools::InputSchema {
        crate::mcp::tools::InputSchema::new(vec![])
    }
}

/// Tool for executing dynamically discovered tools by name.
///
/// This tool acts as a proxy that can invoke any tool in the system, including
/// those added after the agent's initial tool discovery. It's designed to work
/// in conjunction with [`LatestTools`] to enable full dynamic tool usage.
///
/// # Parameters
///
/// - `tool_name` (required): The name of the tool to execute
/// - `params` (optional): Parameters to pass to the target tool as a JSON object
///
/// # Error Handling
///
/// Returns an error if:
/// - The `tool_name` parameter is missing
/// - The specified tool doesn't exist
/// - The target tool returns an error
///
pub struct RunLatestTool;

impl Tool for RunLatestTool {
    fn name(&self) -> &str {
        "run_latest_tool"
    }

    fn description(&self) -> &str {
        "Runs a tool discovered by the `latest_tools` tool.

        This tool may be able to run tools that were added after the agent started.
        "
    }

    fn input_schema(&self) -> InputSchema {
        InputSchema::new(vec![
            crate::mcp::tools::Argument::new(
                "tool_name".to_string(),
                "string".to_string(),
                "Name of the tool to run".to_string(),
                true,
            ),
            crate::mcp::tools::Argument::new(
                "params".to_string(),
                "object".to_string(),
                "Parameters for the tool".to_string(),
                false,
            ),
        ])
    }

    fn call(&self, params: HashMap<String, Value>) -> Result<ToolCallResponse, ToolCallError> {
        let tool_name;
        if let Some(name) = params.get("tool_name").and_then(|v| v.as_str()) {
            tool_name = name.to_string();
        } else {
            return Err(ToolCallError::new(vec![
                "Missing required parameter: tool_name".into(),
            ]));
        }
        let tool_arguments = params
            .get("params")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        //convert to hashmap
        let tool_arguments: HashMap<String, Value> = tool_arguments.into_iter().collect();

        let tool_params = ToolCallParams::new(tool_name, tool_arguments);
        let r = crate::mcp::tools::call_imp(tool_params);
        r.map_err(|e| ToolCallError::new(vec![format!("Error calling tool: {:?}", e).into()]))
    }
}
