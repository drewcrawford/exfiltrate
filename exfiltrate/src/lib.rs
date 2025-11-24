//! Exfiltrate is a remote debugging framework for Rust applications.
//!
//! It allows you to inspect and control a running application (even in WASM/browser environments)
//! from a CLI tool.  This is particularly useful when trying to debug programs with agents such as
//! Claude Code, Codex, Gemini, etc.
//!
//! ![logo](../../../art/logo.png)
//!
//! # Overview
//!
//! Exfiltrate provides a simple, self-contained, and embeddable server implementation,
//! primarily motivated by the need to embed in debuggable programs. It is designed to be
//! easy to use, easy to extend with custom commands, and easy to integrate with existing Rust codebases,
//!
//! Unlike traditional debuggers (gdb, lldb) which require ptrace/OS support, exfiltrate
//! works by embedding a small server thread into your application. This allows it to work
//! in constrained environments like WebAssembly, mobile devices, on remote machines, in sandboxes,
//! etc.
//!
//! # Key Features
//!
//! - **No async runtime required**: Uses threads instead of tokio, simplifying integration.
//! - **Embeddable**: Drop into any Rust application for debugging or agent interaction.
//! - **Platform support**: Works on desktop, mobile, and WebAssembly (with limitations).
//! - **Proxy architecture**: Enables remote debugging of browser/WASM apps via WebSockets.
//! - **Privacy-aware logging**: Integration with `logwise` for controlled log capture.
//!
//! # Use Cases
//!
//! Exfiltrate is the answer to these frequently-asked questions:
//!
//! * How can I quickly expose internal state or operations of my program to a CLI?
//! * How can I add a custom debug command into debug builds of my program?
//! * How can I interact with my program running in a foreign environment, like a mobile app or browser?
//! * How can I steer an LLM agent to reason about my program's state at runtime?
//!
//! # Quick start
//!
//! claude "Run the exfiltrate command, then integrate the library into my program."
//!
//! ## Progressive disclosure
//!
//! A key design philosophy is to use a feature similar to [agent skills](https://code.claude.com/docs/en/skills)
//! to progressively disclose information useful to a task.  When exfiltrate starts up, it provides
//! a helpful menu of topics that can be perused by either humans or agents at their leisure.
//!
//! ## Agent use
//!
//! I recommend instructing agents explicitly to use the exfiltrate command prior to debugging a Rust
//! program.
//!
//! # Less-Quick Start
//!
//! ## Basic Usage
//!
//! 1.  Add `exfiltrate` as a dependency.
//! 2.  Call [`exfiltrate::begin()`](begin) at the start of your program.
//! 3.  Use the `exfiltrate` CLI to connect and run commands.
//!
//! ## Implementing a Custom Command
//!
//! ```rust
//! use exfiltrate::command::{Command, Response};
//!
//! struct HelloCommand;
//!
//! impl Command for HelloCommand {
//!     fn name(&self) -> &'static str {
//!         "hello"
//!     }
//!
//!     fn short_description(&self) -> &'static str {
//!         "Greets a user"
//!     }
//!
//!     fn full_description(&self) -> &'static str {
//!         "Greets a user. Usage: hello [name]"
//!     }
//!
//!     fn execute(&self, args: Vec<String>) -> Result<Response, Response> {
//!         let name = args.get(0).map(|s| s.as_str()).unwrap_or("World");
//!         Ok(format!("Hello, {}!", name).into())
//!     }
//! }
//!
//! // Register the command
//! exfiltrate::add_command(HelloCommand);
//! ```
//!
//! # Architecture
//!
//! ## Why threads?
//!
//! Many Rust networking libraries depend on `tokio` or other async runtimes. This makes sense for
//! high-concurrency servers, but it adds significant weight and complexity when you just want
//! to debug a program.
//!
//! This codebase has no dependency on `tokio`. Instead, it just uses threads. Threads for everyone.
//!
//! ## WebAssembly Support
//!
//! WebAssembly applications running in a browser cannot open raw TCP sockets. To support debugging
//! these applications, `exfiltrate` uses a proxy architecture:
//!
//! 1.  The WASM application connects to a local proxy (`exfiltrate_proxy`) via WebSockets.
//! 2.  The `exfiltrate` CLI connects to the same proxy via TCP.
//! 3.  The proxy bridges the connection, allowing the CLI to control the WASM app as if it were local.
//!
//! # Feature Flags
//!
//! - `logwise` - Enables integration with the `logwise` logging framework for log capture.
//!
//! # Response Types
//!
//! Commands can return different response types:
//!
//! - **String** - Text output (most common)
//! - **Files** - Binary files via [`FileInfo`](command::FileInfo)
//! - **Images** - RGBA images via [`ImageInfo`](command::ImageInfo)
//!
//! For file and image responses, use the types from [`command`] module. Images use
//! [`RGBA8`](rgb::RGBA8) from the re-exported [`rgb`] crate.
//!
//! For detailed examples of all response types, run `exfiltrate help custom_commands` in the CLI.

#[cfg(feature = "logwise")]
mod logwise;

/// Re-export of the [`rgb`](https://docs.rs/rgb) crate for image pixel types.
///
/// Use [`rgb::RGBA8`] when constructing [`ImageInfo`](command::ImageInfo) responses.
pub use rgb;

mod commands;
mod wire;

use crate::commands::register_commands;
use exfiltrate_internal::command::Command;

/// Initializes the exfiltrate debugging server.
///
/// This function should be called as early as possible in your application's lifecycle
/// (e.g., at the start of `main`). It starts the background server thread (or WASM worker)
/// that listens for connections from the CLI.
///
/// # Example
///
/// ```rust
///     #[cfg(feature = "exfiltrate")]
///     exfiltrate::begin();
///
///     // ... rest of your application
/// ```
pub fn begin() {
    #[cfg(feature = "logwise")]
    {
        logwise::begin_log_capture();
    }
    register_commands();
    use std::ops::Deref;
    _ = crate::wire::server::SERVER.deref();
}

/// Registers a custom command with the exfiltrate server.
///
/// Custom commands allow you to expose application-specific state or actions
/// to the CLI.
///
/// # Example
///
/// ```rust
/// use exfiltrate::command::{Command, Response};
///
/// struct MyCommand;
/// impl Command for MyCommand {
///     fn name(&self) -> &'static str { "my_command" }
///     fn short_description(&self) -> &'static str { "Does something cool" }
///     fn full_description(&self) -> &'static str { "Does something cool..." }
///     fn execute(&self, _args: Vec<String>) -> Result<Response, Response> {
///         Ok("Cool!".into())
///     }
/// }
///
/// exfiltrate::add_command(MyCommand);
/// ```
pub fn add_command<C: Command>(command: C) {
    crate::commands::COMMANDS
        .lock_sync_write()
        .push(Box::new(command));
}

/// Re-exports of types needed to implement custom commands.
pub mod command {
    pub use exfiltrate_internal::command::{Command, FileInfo, ImageInfo, Response};
}
