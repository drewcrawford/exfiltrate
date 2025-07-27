use std::io::Write;
use std::net::TcpStream;
use std::sync::{Arc, LazyLock, Mutex};
use std::sync::atomic::AtomicBool;

struct Logger {
    logged: AtomicBool,
    senders: Vec<Arc<Mutex<TcpStream>>>,
}

impl Logger {
    pub fn new() -> Self {
        Logger {
            senders: Vec::new(),
            logged: AtomicBool::new(false),
        }
    }

    pub fn log(&self, message: &str) {
        self.logged.store(true, std::sync::atomic::Ordering::Relaxed);
        // Log the message
        println!("{}", message);
        // todo!();
        // for sender in &mut self.senders {
        //     // Send the message to all registered senders
        //     if let Err(e) = sender.lock().unwrap().write_all(format!("{}\n", message).as_bytes()) {
        //         eprintln!("Failed to send log message: {}", e);
        //     }
        // }
    }

    pub fn register_sender(&mut self, mut sender: Arc<Mutex<TcpStream>>) {
        if self.logged.load(std::sync::atomic::Ordering::Relaxed) {
            sender.lock().unwrap().write("You are connecting after messages have already been logged; to retain earlier logs rerun with EXFILTRATE_BUFFER=1".as_bytes()).unwrap();
        }
        self.senders.push(sender);
    }
}

const GLOBAL_LOGGER: LazyLock<Mutex<Logger>> = LazyLock::new(|| Mutex::new(Logger::new()));

pub fn register_sender(sender: Arc<Mutex<TcpStream>>) {
    GLOBAL_LOGGER.lock().unwrap().register_sender(sender);
}
pub fn log(message: &str) {
    GLOBAL_LOGGER.lock().unwrap().log(message);
}
