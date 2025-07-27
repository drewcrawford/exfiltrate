use std::net::ToSocketAddrs;

mod core;

pub mod logging;


pub fn exfiltrate_up<A: ToSocketAddrs>(addr: A) {
    core::exfiltrate_up(addr);
}