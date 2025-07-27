use std::net::ToSocketAddrs;

mod core;



pub fn exfiltrate_up<A: ToSocketAddrs>(addr: A) {
    core::exfiltrate_up(addr);
}