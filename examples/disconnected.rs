fn main() {
    let proxy = exfiltrate::transit::transit_proxy::TransitProxy::new();
    let server = exfiltrate::transit::http::Server::new("127.0.0.1:1984",proxy);
    std::thread::sleep(std::time::Duration::from_secs(1000));
}