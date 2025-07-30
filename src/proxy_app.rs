#![cfg(feature="transit")]

use exfiltrate::transit::transit_proxy::TransitProxy;

fn main() {
    let transit_proxy = TransitProxy::new();
    let _proxy = exfiltrate::transit::http::Server::new("127.0.0.1:1984", transit_proxy);
    // let _proxy = exfiltrate::transit::stdio::Server::new(transit_proxy);
    std::thread::park();
}