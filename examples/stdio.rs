#[cfg(not(target_arch = "wasm32"))]
use std::time;
#[cfg(target_arch = "wasm32")]
use web_time as time;

fn main() {
    let _server = exfiltrate::transit::stdio::Server::new(
        exfiltrate::transit::transit_proxy::TransitProxy::new(),
    );
    std::thread::sleep(time::Duration::from_secs(1_000));
}
