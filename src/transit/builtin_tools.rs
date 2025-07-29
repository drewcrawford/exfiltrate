use std::collections::HashMap;
use std::sync::LazyLock;
use crate::tools::{Tool, ToolCallParams, ToolCallResponse, ToolInfo, ToolList};


//These tools are only available in the proxy application.  They are NOT available in the target application;
//the proxy's version is always used.
static PROXY_ONLY_TOOLS: LazyLock<Vec<Box<dyn Tool>>> = LazyLock::new(|| {
    vec![
        #[cfg(feature="logwise")]
        Box::new(crate::transit::log_proxy::LogwiseRead),

    ]
});

/**
Returns a list of tools that are available in the proxy application.

This includes both the tools that are proxy-only and tools that are shared.
*/
pub fn proxy_tools() -> ToolList {
    let mut tools = crate::tools::SHARED_TOOLS.iter()
        .chain(PROXY_ONLY_TOOLS.iter())
        .map(|tool| ToolInfo::from_tool(tool.as_ref()))
        .collect::<Vec<_>>();
    ToolList { tools }
}

/**
Returns a list of tools that are only available in the proxy application.
These tools are not available in the target application.
*/
pub fn proxy_only_tools() -> ToolList {
    let tools = PROXY_ONLY_TOOLS.iter()
        .map(|tool| ToolInfo::from_tool(tool.as_ref()))
        .collect::<Vec<_>>();
    ToolList { tools }
}

/**
Calls a tool on the proxy application.

This function only considers tools in the proxy (e.g. for shared tools we do not consider
the target application version, only the proxy's version).
*/
pub fn call_proxy_tool(params: ToolCallParams) -> Result<ToolCallResponse, crate::jrpc::Error> {
    //try proxy tools first
    //convert to hashmap
    let hashmap: HashMap<_, _> = params.arguments;

    if let Some(tool) = PROXY_ONLY_TOOLS.iter().find(|tool| tool.name() == params.name) {
        match tool.call(hashmap) {
            Ok(response) => Ok(response),
            Err(e) => Ok(e.into_response())
        }
    }
    else if let Some(tool) = crate::tools::SHARED_TOOLS.iter().find(|tool| tool.name() == params.name) {
        match tool.call(hashmap) {
            Ok(response) => Ok(response),
            Err(e) => Ok(e.into_response())
        }
    } else {
        Err(crate::jrpc::Error::invalid_params("No tool found with the given name".to_string()))
    }
}

/**
Calls a tool that is only available in the proxy application.
*/
pub fn call_proxy_only_tool(params: ToolCallParams) -> Result<ToolCallResponse, crate::jrpc::Error> {
    //try proxy tools first
    //convert to hashmap
    let hashmap: HashMap<_, _> = params.arguments;
    if let Some(tool) = PROXY_ONLY_TOOLS.iter().find(|tool| tool.name() == params.name) {
        match tool.call(hashmap) {
            Ok(response) => Ok(response),
            Err(e) => Ok(e.into_response())
        }
    } else {
        Err(crate::jrpc::Error::invalid_params("No tool found with the given name".to_string()))
    }
}