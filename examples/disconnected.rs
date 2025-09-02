// SPDX-License-Identifier: MIT OR Apache-2.0
#[cfg(not(target_arch = "wasm32"))]
use std::time;
#[cfg(target_arch = "wasm32")]
use web_time as time;

fn main() {
    let proxy = exfiltrate::transit::transit_proxy::TransitProxy::new();
    let _server = exfiltrate::transit::http::Server::new("127.0.0.1:1984", proxy);
    std::thread::sleep(time::Duration::from_secs(1000));
}
