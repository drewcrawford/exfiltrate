
pub fn main() {
    exfiltrate::logwise::begin_capture();
    logwise::info_sync!("LOG MESSAGE 1");
    let proxy = exfiltrate::transit::transit_proxy::TransitProxy::new();
    let server = exfiltrate::transit::stdio::Server::new(proxy);
    logwise::info_sync!("LOG MESSAGE 2");
    std::thread::sleep(std::time::Duration::from_secs(1_000));
}