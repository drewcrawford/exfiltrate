#![cfg(target_arch = "wasm32")]

use std::fmt::Display;
use crate::bidirectional_proxy::Transport;

#[derive(Debug)]
pub struct WebsocketAdapter {

}

#[derive(Debug)]
pub enum Error {
    CantConnect(String),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            _ => write!(f, "WebsocketAdapter error"),
        }
    }
}

const ADDR: &str = "ws://localhost:1986";

impl WebsocketAdapter {
    pub fn new() -> Result<Self, Error> {
        //put ws communication on its own thread
        let (s,r) = std::sync::mpsc::channel();

        crate::sys::thread::Builder::new()
            .name("exfiltrate::WebsocketAdapter".to_owned())
            .spawn(move || {
                let ws = web_sys::WebSocket::new(ADDR);
                match ws {
                    Ok(ws) => {
                        s.send(Ok(())).unwrap();
                    }
                    Err(e) => {
                        s.send(Err(Error::CantConnect(format!("{:?}",e)))).unwrap();
                    }
                }
            }).unwrap();
        match r.recv().unwrap() {
            Ok(_) => {
                Ok(WebsocketAdapter {})
            }
            Err(e) => {
                logwise::error_sync!("WebsocketAdapter: Failed to connect with error: {e}", e=logwise::privacy::LogIt(&e));
                Err(e)
            }
        }
    }
}

impl Transport for WebsocketAdapter {
    fn write_block(&mut self, data: &[u8]) -> Result<(), crate::bidirectional_proxy::Error> {
        logwise::error_sync!("WebsocketAdapter::write_block is not implemented yet");
        todo!("Implement WebsocketAdapter::write_block");
    }

    fn flush(&mut self) -> Result<(), crate::bidirectional_proxy::Error> {
        logwise::error_sync!("WebsocketAdapter::flush is not implemented yet");
        todo!("Implement WebsocketAdapter::flush");
    }

    fn read_nonblock(&mut self, buf: &mut [u8]) -> Result<usize, crate::bidirectional_proxy::Error> {
        logwise::error_sync!("WebsocketAdapter::read_nonblock is not implemented yet");
        todo!("Implement WebsocketAdapter::read_nonblock");
    }
}