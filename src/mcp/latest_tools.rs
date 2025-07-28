use std::any::Any;
use std::collections::HashMap;
use serde_json::Value;
use crate::tools::{InputSchema, Tool, ToolCallError, ToolCallParams, ToolCallResponse};

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

    fn call(&self, params: std::collections::HashMap<String, serde_json::Value>) -> Result<crate::tools::ToolCallResponse, crate::tools::ToolCallError> {
        let tools = crate::mcp::tools::list_int();
        let text = serde_json::to_string(&tools).unwrap();
        Ok(crate::tools::ToolCallResponse::new(vec![text.into()]))
    }

    fn input_schema(&self) -> crate::mcp::tools::InputSchema {
        crate::mcp::tools::InputSchema::new(vec![])
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}


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
            crate::tools::Argument::new("tool_name".to_string(), "string".to_string(), "Name of the tool to run".to_string(), true),
            crate::tools::Argument::new("params".to_string(), "object".to_string(), "Parameters for the tool".to_string(), false),
        ])
    }

    fn call(&self, params: HashMap<String, Value>) -> Result<ToolCallResponse, ToolCallError> {
        let tool_name;
        if let Some(name) = params.get("tool_name").and_then(|v| v.as_str()) {
            tool_name = name.to_string();
        } else {
            return Err(ToolCallError::new(vec!["Missing required parameter: tool_name".into()]));
        }
        let tool_arguments = params.get("params")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        //convert to hashmap
        let tool_arguments: HashMap<String, Value> = tool_arguments.into_iter()
            .map(|(k, v)| (k, v))
            .collect();

        let tool_params = ToolCallParams::new(tool_name, tool_arguments);
        let r = crate::mcp::tools::call_imp(tool_params);
        r.map_err(|e| ToolCallError::new(vec![format!("Error calling tool: {:?}", e).into()]))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

