use crate::transit::transit_proxy::TransitProxy;
use std::io::Write;
pub struct Server {
    proxy: TransitProxy,
}

impl Server {
    pub fn new(proxy: TransitProxy) -> Self {
        proxy.bind(move |msg| {
            let mut stdin = std::io::stdin();
            let mut stdout = std::io::stdout();
            loop {
                let bytes = serde_json::to_vec(&msg).unwrap();
                stdout.write_all(&bytes).unwrap();
                stdout.write_all(b"\n").unwrap();
                stdout.flush().unwrap();
            }
        });
        std::thread::Builder::new()
            .name("exfiltrate::stdio".to_string())
            .spawn(|| {
                let mut stdin = std::io::stdin();
                loop {
                    let mut buffer = String::new();
                    if stdin.read_line(&mut buffer).is_err() {
                        eprintln!("Failed to read from stdin, exiting...");
                        break;
                    }
                    let r = serde_json::from_str::<crate::jrpc::Request>(&buffer);

                    eprintln!("Received request from stdin: {:?}", r);
                    match r {
                        Ok(request) => { todo!() },
                        Err(e) => todo!(),
                    }
                }
            });
        eprintln!("Proxy started on stdin/stdout");
        Server {proxy}
    }


}