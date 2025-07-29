use crate::transit::transit_proxy::TransitProxy;
use std::io::Write;
pub struct Server {
}

impl Server {
    pub fn new(mut proxy: TransitProxy) -> Self {
        proxy.bind(move |msg| {
            let mut stdout = std::io::stdout();
            let bytes = serde_json::to_vec(&msg).unwrap();
            stdout.write_all(&bytes).unwrap();
            stdout.write_all(b"\n").unwrap();
            stdout.flush().unwrap();
        });
        std::thread::Builder::new()
            .name("exfiltrate::stdio".to_string())
            .spawn(move || {
                let stdin = std::io::stdin();
                loop {
                    let mut buffer = String::new();
                    if stdin.read_line(&mut buffer).is_err() {
                        eprintln!("Failed to read from stdin, exiting...");
                        break;
                    }
                    eprintln!("Received data from stdin: {}", buffer);
                    let buffer = buffer.trim().as_bytes();
                    match proxy.received_data(buffer) {
                        Some(response) => {
                            let as_bytes = serde_json::to_vec(&response).unwrap();
                            let mut stdout = std::io::stdout();
                            stdout.write_all(&as_bytes).unwrap();
                            stdout.write_all(b"\n").unwrap();
                            stdout.flush().unwrap();
                        }
                        None => {
                            //nothing?
                        }
                    }
                }
            }).unwrap();
        eprintln!("Proxy started on stdin/stdout");
        Server {}
    }


}