use std::net::ToSocketAddrs;

mod core;
mod http;
mod jrpc;
pub mod mcp;


pub fn exfiltrate_up<A: ToSocketAddrs>(addr: A) {
    // Create a new server instance that listens on the specified address
    let server = crate::http::Server::new(addr);
    std::mem::forget(server);

}

pub use mcp::tools;