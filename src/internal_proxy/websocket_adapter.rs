#![cfg(target_arch = "wasm32")]

use std::fmt::Display;
use std::time::Duration;
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
    //this function must absolutely be async.
    //spinloop-based approaches empirically do not work in the browser.
    pub async fn new() -> Result<Self, Error> {
        //put ws communication on its own thread
        let (s,r) = std::sync::mpsc::channel();

        crate::sys::thread::Builder::new()
            .name("exfiltrate::WebsocketAdapter".to_owned())
            .spawn(move || {
                logwise::info_sync!("Thread started");
                let ws = web_sys::WebSocket::new(ADDR);
                logwise::info_sync!("Got result {ws}", ws=logwise::privacy::LogIt(&ws));
                match ws {
                    Ok(ws) => {
                        s.send(Ok(())).unwrap();
                    }
                    Err(e) => {
                        s.send(Err(Error::CantConnect(format!("{:?}",e)))).unwrap();
                    }
                }
            }).unwrap();
        //spin loop
        let mut perf = None;
        loop {
            // logwise::info_sync!("Will spin until WebsocketAdapter is connected");
            let r = r.try_recv();
            // logwise::info_sync!("Completed recv");
            match r {
                Ok(inner) => {
                    match inner {
                        Ok(()) => {
                            return Ok(WebsocketAdapter {})

                        }
                        Err(e) => {
                            logwise::error_sync!("WebsocketAdapter: Failed to connect with error: {e}", e=logwise::privacy::LogIt(&e));
                            return Err(e)
                        }
                    }
                }
                Err(e) => {
                    // logwise::info_sync!("WebsocketAdapter: WebsocketAdapter: error {e}", e=logwise::privacy::LogIt(&e));
                    #[cfg(feature = "logwise")] {
                        if perf.is_none() {
                            perf = Some(logwise::perfwarn_begin!("exfiltrate::WebsocketAdapter::new spin"));
                        }
                    }
                    #[cfg(not(feature = "logwise"))] {
                        perf = Some((())); //infer a type
                    }
                }
            }
        }
    //     Ok(WebsocketAdapter {})
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