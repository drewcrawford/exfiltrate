/*!
An embeddable MCP server for Rust.

exfiltrate provides a simple, self-contained and embeddable MCP server implementation,
primarily motivated by the need to embed in debuggable programs.  It is designed to be
easy to use, easy to extend, and easy to integrate with existing Rust codebases.

exfiltrate is the answer to these frequently-asked questions:

* How can I quickly sketch or prototype a new MCP tool?
* How can I add a custom MCP tool into debug builds of my program?
* How can I expose internal state or operations of my program to an agent?
* How can agents interact with my program running in a foreign environment, like a mobile app or a
browser?


exfiltrate is also the answer to these, less-frequently-asked questions:

# Since many agents freeze the list of MCP tools on startup, how can I do workloads that heavily rely on starting/stopping my program?

In theory, the MCP protocol allows you to push updates when tools change.  In practice, support for
this is often unimplemented.

But there's an elegant workaround: write a "tell me the latest tools" tool, and a "run another tool
by name" tool, boom, dynamic tool discovery and use by all agents.  Tools that are built into the
proxy persist whereas tools built into your program come and go.

# Why does the official MCP SDK depend on tokio?

Probably because that makes sense for internet-deployed MCP servers but it makes no sense for
debugging an arbitrary program that doesn't even work which is why you're debugging it.

This codebase has no dependency on tokio or the official SDK.  Instead it just uses threads.
Threads for everyone.

# Ok, but I'm doing mobile or WebAssembly, how do I run a server there?

By proxying it of course.  Your browser opens a websocket to the proxy application which sends
it to Claude.

Since the async story is bad there too, I just wrote a ground-up WebSocket implementation
with threads.  Threads for everyone.

*/
mod bidirectional_proxy;
mod internal_proxy;
pub mod jrpc;
mod logging;
#[cfg(feature = "logwise")]
pub mod logwise;
pub mod mcp;
pub mod messages;
mod once_nonlock;
mod sys;
#[cfg(feature = "transit")]
pub mod transit;

pub use mcp::tools;
