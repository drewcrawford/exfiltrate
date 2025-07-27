use std::net::ToSocketAddrs;

mod http;
mod jrpc;
pub mod mcp;

pub fn exfiltrate_up<A: ToSocketAddrs>(addr: A) {
    // Create a new server instance that listens on the specified address
    let server = crate::core::http::Server::new(addr);
    std::mem::forget(server);

}