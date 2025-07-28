fn main() {
    let s = exfiltrate::transit::stdio::Server::new(
        exfiltrate::transit::transit_proxy::TransitProxy::new(),
    );
    std::thread::sleep(std::time::Duration::from_secs(1_000));
}