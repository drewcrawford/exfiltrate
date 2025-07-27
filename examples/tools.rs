fn main() {
    let server = exfiltrate::exfiltrate_up("127.0.0.1:1984");
    std::thread::sleep(std::time::Duration::from_secs(1000));
}