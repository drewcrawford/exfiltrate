# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

exfiltrate is an embeddable Model Context Protocol (MCP) server for Rust that provides a self-contained MCP implementation without async runtime dependencies. It's designed for embedding in debuggable programs and uses threads instead of tokio.

## Build Commands

```bash
# Build the library
cargo build

# Build with all features (transit and logwise)
cargo build --all-features

# Build specific features
cargo build --features transit
cargo build --features logwise

# Build the proxy binary (requires transit feature)
cargo build --bin proxy --features transit

# Run tests
cargo test
cargo test --all-features

# Run a single test
cargo test test_name

# Build and run examples (requires transit feature for most)
cargo run --example tools --features transit
cargo run --example stdio --features transit
cargo run --example log_exfiltration --features "transit logwise"

# Linting and formatting
cargo clippy
cargo fmt

# Build for WebAssembly (limited functionality)
cargo build --target wasm32-unknown-unknown
```

## Architecture

### Core Modules

- **`src/mcp.rs` & `src/mcp/`**: Core MCP protocol implementation
  - `tools.rs`: Tool registry and base trait definitions
  - `latest_tools.rs`: Dynamic tool discovery mechanism

- **`src/transit.rs` & `src/transit/`**: Transit proxy system (feature-gated with `transit`)
  - `transit_proxy.rs`: Main proxy implementation
  - `http.rs`: HTTP server for WebSocket connections
  - `stdio.rs`: Standard I/O transport
  - `builtin_tools.rs`: Built-in proxy tools for dynamic discovery
  - `log_proxy.rs`: Log forwarding proxy

- **`src/logwise.rs`**: Integration with logwise logging framework (feature-gated with `logwise`)

- **`src/jrpc.rs`**: JSON-RPC implementation

- **`src/bidirectional_proxy.rs`**: Bidirectional proxy for routing messages

- **`src/internal_proxy.rs`**: Internal proxy for in-process communication

### Key Design Patterns

1. **No Async Runtime**: Uses threads and blocking I/O instead of tokio/async-await
2. **Dynamic Tool Discovery**: Works around agent limitations through "latest_tools" and "run_tool" pattern
3. **Proxy Architecture**: Enables debugging in constrained environments (mobile, WASM)
4. **Thread-based Concurrency**: All concurrent operations use std::thread

## Working with Tools

Tools implement the `exfiltrate::mcp::tools::Tool` trait:

```rust
pub trait Tool {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> InputSchema;
    fn call(&self, params: HashMap<String, serde_json::Value>) 
        -> Result<ToolCallResponse, ToolCallError>;
}
```

Register tools using `exfiltrate::mcp::tools::add_tool(Box::new(YourTool))`.

## Logging with logwise

When using logwise (see user instructions in ~/.claude/CLAUDE.md):
- Use `logwise::info_sync!`, `logwise::warn_sync!`, etc. for synchronous logging
- Complex types require `logwise::privacy::LogIt` wrapper
- Enable log capture with `exfiltrate::logwise::begin_capture()` if using the logwise feature

## Feature Flags

- `transit`: Enables transit proxy system (not available on wasm32)
- `logwise`: Enables logwise logging integration

## Platform Considerations

- **Desktop (Linux/macOS/Windows)**: Full functionality
- **WebAssembly**: Limited to WebSocket client mode, no server capabilities
- **Mobile**: Use transit proxy for remote debugging