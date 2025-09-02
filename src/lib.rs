/*!
An embeddable Model Context Protocol (MCP) server for Rust.

![logo](../../../art/logo.png)

exfiltrate provides a simple, self-contained and embeddable MCP server implementation,
primarily motivated by the need to embed in debuggable programs. It is designed to be
easy to use, easy to extend, and easy to integrate with existing Rust codebases.

# Overview

The Model Context Protocol (MCP) enables AI models and agents to interact with external
systems through a standardized JSON-RPC interface. exfiltrate implements this protocol
without requiring async runtimes like tokio, making it suitable for embedding in any
Rust application, including those running in constrained environments.

# Key Features

- **No async runtime required**: Uses threads instead of tokio, simplifying integration
- **Embeddable**: Drop into any Rust application for debugging or agent interaction
- **Dynamic tool discovery**: Work around agent limitations with built-in tool discovery
- **Platform support**: Works on desktop, mobile, and WebAssembly (with limitations)
- **Proxy architecture**: Enables remote debugging and tool persistence
- **Privacy-aware logging**: Integration with logwise for controlled log capture

# Use Cases

exfiltrate is the answer to these frequently-asked questions:

* How can I quickly sketch or prototype a new MCP tool?
* How can I add a custom MCP tool into debug builds of my program?
* How can I expose internal state or operations of my program to an agent?
* How can agents interact with my program running in a foreign environment, like a mobile app or browser?

# Quick Start

## Basic Tool Implementation

```
use exfiltrate::mcp::tools::{Tool, InputSchema, Argument, ToolCallResponse, ToolCallError};
use std::collections::HashMap;

// Define a simple tool
struct HelloTool;

impl Tool for HelloTool {
    fn name(&self) -> &str {
        "hello"
    }
    
    fn description(&self) -> &str {
        "Greets a user by name"
    }
    
    fn input_schema(&self) -> InputSchema {
        InputSchema::new(vec![
            Argument::new(
                "name".to_string(),
                "string".to_string(),
                "Name to greet".to_string(),
                true
            ),
        ])
    }
    
    fn call(&self, params: HashMap<String, serde_json::Value>) 
        -> Result<ToolCallResponse, ToolCallError> {
        let name = params.get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolCallError::new(vec!["Missing name parameter".into()]))?;
        
        Ok(ToolCallResponse::new(vec![
            format!("Hello, {}!", name).into()
        ]))
    }
}

// Register the tool
exfiltrate::mcp::tools::add_tool(Box::new(HelloTool));
```


## Starting a Transit Proxy

```no_run
# // don't bind port in doctests
# #[cfg(feature = "transit")]
# {
use exfiltrate::transit::{transit_proxy::TransitProxy, http::Server};

// Create and start an HTTP proxy server
let proxy = TransitProxy::new();
let server = Server::new("127.0.0.1:1984", proxy);

// Server runs in background threads
// In a real application, keep the main thread alive
std::thread::sleep(std::time::Duration::from_millis(10));
drop(server); // Cleanup for doctest
# }
```

## Working with Logwise Integration

```
# #[cfg(feature = "logwise")]
# {
// Enable log capture to forward logs through MCP
// Note: This would typically be called once at program start
// exfiltrate::logwise::begin_capture();

// Logs using logwise syntax will be captured
// Example (not executed in doctest):
// logwise::info_sync!("Application started", version="1.0.0");
// logwise::warn_sync!("Low memory", available_mb=256);
# }
```

# Best Practices

## Error Handling

When implementing tools, always validate input parameters and provide clear error messages:

```
use exfiltrate::mcp::tools::{Tool, ToolCallResponse, ToolCallError};
use std::collections::HashMap;

struct SafeTool;

impl Tool for SafeTool {
    fn name(&self) -> &str { "safe_divide" }
    fn description(&self) -> &str { "Safely divides two numbers" }
    
    fn input_schema(&self) -> exfiltrate::mcp::tools::InputSchema {
        use exfiltrate::mcp::tools::{InputSchema, Argument};
        InputSchema::new(vec![
            Argument::new("dividend".into(), "number".into(), "Number to divide".into(), true),
            Argument::new("divisor".into(), "number".into(), "Number to divide by".into(), true),
        ])
    }
    
    fn call(&self, params: HashMap<String, serde_json::Value>) 
        -> Result<ToolCallResponse, ToolCallError> {
        // Validate and extract parameters with clear error messages
        let dividend = params.get("dividend")
            .ok_or_else(|| ToolCallError::new(vec!["Missing 'dividend' parameter".into()]))?
            .as_f64()
            .ok_or_else(|| ToolCallError::new(vec!["'dividend' must be a number".into()]))?;
            
        let divisor = params.get("divisor")
            .ok_or_else(|| ToolCallError::new(vec!["Missing 'divisor' parameter".into()]))?
            .as_f64()
            .ok_or_else(|| ToolCallError::new(vec!["'divisor' must be a number".into()]))?;
        
        // Check for division by zero
        if divisor == 0.0 {
            return Err(ToolCallError::new(vec!["Cannot divide by zero".into()]));
        }
        
        let result = dividend / divisor;
        Ok(ToolCallResponse::new(vec![format!("{}", result).into()]))
    }
}
```

## Tool Naming Conventions

- Use lowercase snake_case for tool names
- Choose descriptive names that clearly indicate the tool's purpose
- Avoid generic names like "process" or "handle"
- Prefix related tools with a common namespace (e.g., `file_read`, `file_write`)

## Performance Considerations

- Tools are executed synchronously - keep operations fast or consider background processing
- For long-running operations, consider returning a status and providing a separate query tool
- The system uses threads, not async/await, so blocking operations will block the thread

# Architecture

exfiltrate is also the answer to these less-frequently-asked questions:

## Since many agents freeze the list of MCP tools on startup, how can I do workloads that heavily rely on starting/stopping my program?

In theory, the MCP protocol allows you to push updates when tools change. In practice, support for
this is often unimplemented.

But there's an elegant workaround: write a "tell me the latest tools" tool, and a "run another tool
by name" tool, boom, dynamic tool discovery and use by all agents. Tools that are built into the
proxy persist whereas tools built into your program come and go.

## Why does the official MCP SDK depend on tokio?

Probably because that makes sense for internet-deployed MCP servers but it makes no sense for
debugging an arbitrary program that doesn't even work which is why you're debugging it.

This codebase has no dependency on tokio or the official SDK. Instead it just uses threads.
Threads for everyone.

## Ok, but I'm doing mobile or WebAssembly, how do I run a server there?

By proxying it of course. Your browser opens a websocket to the proxy application which sends
it to Claude.

Since the async story is bad there too, I just wrote a ground-up WebSocket implementation
with threads. Threads for everyone.

# Feature Flags

- `transit` - Enables the transit proxy system for remote debugging (not available on wasm32)
- `logwise` - Enables integration with the logwise logging framework for log capture

# Module Organization

- [`mcp`] - Model Context Protocol core implementation
- [`messages`] - Inter-component message types
- `transit` - Transit proxy system (requires `transit` feature, not available on wasm32)
- `logwise` - Logwise logging integration (requires `logwise` feature)

*/
mod bidirectional_proxy;
mod internal_proxy;
mod jrpc;
mod logging;
#[cfg(feature = "logwise")]
pub mod logwise;
pub mod mcp;
mod once_nonlock;
mod sys;
#[cfg(feature = "transit")]
pub mod transit;

