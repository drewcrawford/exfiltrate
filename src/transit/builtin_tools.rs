//! Built-in tools available only in the transit proxy.
//!
//! This module provides tools that are exclusive to the proxy application and are not
//! available in the target application. These tools enable the proxy to provide additional
//! functionality beyond what the target application offers, such as log inspection and
//! proxy-specific operations.
//!
//! # Overview
//!
//! The transit proxy sits between clients and target applications, intercepting and
//! augmenting the Model Context Protocol (MCP) communication. This module manages the
//! proxy's exclusive toolset, allowing it to provide additional functionality that
//! the target application doesn't offer.
//!
//! The proxy can augment the tool set available to clients by:
//! - Providing proxy-only tools (e.g., log inspection tools)
//! - Merging proxy tools with target application tools
//! - Intercepting and handling specific tool calls locally
//!
//! # Tool Categories
//!
//! ## Proxy-Only Tools
//! Tools that exist only in the proxy and handle proxy-specific functionality:
//! - `LogwiseRead`: Read captured logs from logwise
//! - `LogwiseGrep`: Search through captured logs using regular expressions
//!
//! ## Shared Tools
//! Tools that are available in both the proxy and target application. When invoked
//! through the proxy, the proxy's implementation is used.
//!
//! # Architecture
//!
//! The module uses lazy static initialization to register tools at startup. Tools are
//! stored in static collections that are accessed when handling tool discovery and
//! invocation requests.
//!
//! # Feature Flags
//!
//! Some tools are conditionally compiled based on feature flags:
//! - `logwise`: Enables log capture and inspection tools (`LogwiseRead`, `LogwiseGrep`)
//!

use crate::tools::{Tool, ToolCallParams, ToolCallResponse, ToolInfo, ToolList};
use std::collections::HashMap;
use std::sync::LazyLock;

/// Static collection of tools that are only available in the proxy application.
///
/// These tools are NOT available in the target application; the proxy's version
/// is always used. This includes tools for log inspection and other proxy-specific
/// functionality.
static PROXY_ONLY_TOOLS: LazyLock<Vec<Box<dyn Tool>>> = LazyLock::new(|| {
    vec![
        #[cfg(feature = "logwise")]
        Box::new(crate::transit::log_proxy::LogwiseRead),
        #[cfg(feature = "logwise")]
        Box::new(crate::transit::log_proxy::LogwiseGrep),
    ]
});

/// Returns a list of all tools available in the proxy application.
///
/// This function combines proxy-only tools with shared tools to provide the complete
/// set of tools available when using the proxy. The returned list represents all tools
/// that clients can invoke through the proxy, regardless of whether they're implemented
/// in the proxy, the target application, or both.
///
/// # Returns
///
/// A `ToolList` containing metadata for all available tools, including:
/// - Tool names
/// - Descriptions
/// - Input schemas
pub fn proxy_tools() -> ToolList {
    let tools = crate::tools::SHARED_TOOLS
        .iter()
        .chain(PROXY_ONLY_TOOLS.iter())
        .map(|tool| ToolInfo::from_tool(tool.as_ref()))
        .collect::<Vec<_>>();
    ToolList { tools }
}

/// Returns a list of tools that are exclusive to the proxy application.
///
/// These tools are not available in the target application and provide
/// proxy-specific functionality such as log inspection and monitoring.
/// The availability of specific tools depends on compile-time feature flags.
///
/// # Returns
///
/// A `ToolList` containing only the proxy-exclusive tools. This may be empty
/// if no proxy-only tools are compiled in (e.g., when the `logwise` feature is disabled).
///
pub fn proxy_only_tools() -> ToolList {
    let tools = PROXY_ONLY_TOOLS
        .iter()
        .map(|tool| ToolInfo::from_tool(tool.as_ref()))
        .collect::<Vec<_>>();
    ToolList { tools }
}

/// Calls a tool on the proxy application.
///
/// This function executes tools locally in the proxy, considering both proxy-only
/// tools and shared tools. It first checks proxy-only tools, then shared tools.
/// For shared tools, only the proxy's version is used, not the target application's version.
///
/// # Arguments
///
/// * `params` - The tool call parameters including:
///   - `name`: The name of the tool to invoke
///   - `arguments`: A HashMap of parameter names to JSON values
///
/// # Returns
///
/// * `Ok(ToolCallResponse)` - The successful response from the tool
/// * `Err(Error)` - A JSON-RPC error if the tool is not found
///
/// # Error Handling
///
/// Tool execution errors are converted to successful responses with the error flag set,
/// following the MCP protocol convention. Only missing tools result in JSON-RPC errors.
///
pub fn call_proxy_tool(params: ToolCallParams) -> Result<ToolCallResponse, crate::jrpc::Error> {
    //try proxy tools first
    //convert to hashmap
    let hashmap: HashMap<_, _> = params.arguments;

    if let Some(tool) = PROXY_ONLY_TOOLS
        .iter()
        .find(|tool| tool.name() == params.name)
    {
        match tool.call(hashmap) {
            Ok(response) => Ok(response),
            Err(e) => Ok(e.into_response()),
        }
    } else if let Some(tool) = crate::tools::SHARED_TOOLS
        .iter()
        .find(|tool| tool.name() == params.name)
    {
        match tool.call(hashmap) {
            Ok(response) => Ok(response),
            Err(e) => Ok(e.into_response()),
        }
    } else {
        Err(crate::jrpc::Error::invalid_params(format!(
            "No tool found with the name {}",
            params.name
        )))
    }
}

/// Calls a tool that is exclusively available in the proxy application.
///
/// This function only considers proxy-only tools and will fail if the requested
/// tool is not in the proxy-only tool set. This is useful when you want to ensure
/// that a tool is handled by the proxy and not delegated to the target application.
///
/// # Arguments
///
/// * `params` - The tool call parameters including:
///   - `name`: The name of the proxy-only tool to invoke
///   - `arguments`: A HashMap of parameter names to JSON values
///
/// # Returns
///
/// * `Ok(ToolCallResponse)` - The successful response from the proxy-only tool
/// * `Err(Error)` - A JSON-RPC error if the tool is not found in proxy-only tools
pub fn call_proxy_only_tool(
    params: ToolCallParams,
) -> Result<ToolCallResponse, crate::jrpc::Error> {
    //try proxy tools first
    //convert to hashmap
    let hashmap: HashMap<_, _> = params.arguments;
    if let Some(tool) = PROXY_ONLY_TOOLS
        .iter()
        .find(|tool| tool.name() == params.name)
    {
        match tool.call(hashmap) {
            Ok(response) => Ok(response),
            Err(e) => Ok(e.into_response()),
        }
    } else {
        Err(crate::jrpc::Error::invalid_params(format!(
            "No tool found with the name {}",
            params.name
        )))
    }
}
