fn main() {
    let server = exfiltrate::exfiltrate_up("127.0.0.1:1984");
    std::thread::Builder::new()
        .name("logging-thread".to_string()).spawn(|| {
        loop {
            exfiltrate::logging::log("hi there");
            std::thread::sleep(std::time::Duration::from_secs(1));
        }

    });
    std::thread::sleep(std::time::Duration::from_secs(1000));
}