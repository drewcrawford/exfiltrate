#![cfg(target_arch = "wasm32")]

use std::fmt::Display;
use std::time::Duration;
use crate::bidirectional_proxy::Transport;
use wasm_bindgen::JsCast;
use std::sync::{Arc, Mutex};

use wasm_bindgen::closure::Closure;

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

struct OneShot<T> {
    c: Arc<Mutex<Option<r#continue::Sender<T>>>>,
}

impl<T> OneShot<T> {
    fn new(sender: r#continue::Sender<T>) -> Self {
        OneShot {
            c: Arc::new(Mutex::new(Some(sender))),
        }
    }

    fn send_if_needed(&self, value: T) {
        if let Some(sender) = self.c.lock().unwrap().take() {
            sender.send(value);
        }
    }
}

impl<T> Clone for OneShot<T> {
    fn clone(&self) -> Self {
        OneShot {
            c: Arc::clone(&self.c),
        }
    }
}

const ADDR: &str = "ws://localhost:1985";

impl WebsocketAdapter {
    //this function must absolutely be async.
    //spinloop-based approaches empirically do not work in the browser for this API.
    pub async fn new() -> Result<Self, Error> {
        //put ws communication on its own thread
        let (c,f) = r#continue::continuation();

        crate::sys::thread::Builder::new()
            .name("exfiltrate::WebsocketAdapter".to_owned())
            .spawn(move || {
                logwise::info_sync!("Thread started");
                let ws = web_sys::WebSocket::new(ADDR);
                logwise::info_sync!("Created ws {ws}", ws=logwise::privacy::LogIt(&ws));
                let c = OneShot::new(c);
                match ws {
                    Ok(ws) => {
                        let move_c = c.clone();
                        let onopen_callback = Closure::wrap(Box::new(move |event: web_sys::Event| {
                            web_sys::console::log_1(&"WebSocket opened!".into());
                            move_c.send_if_needed(Ok(()));
                        }) as Box<dyn FnMut(_)>);
                        ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
                        onopen_callback.forget(); //leak the closure

                        let move_c = c.clone();
                        let onerror_callback = Closure::wrap(Box::new(move |event: web_sys::ErrorEvent| {
                            web_sys::console::log_1(&"WebSocket error!".into());
                            move_c.send_if_needed(Err(Error::CantConnect(event.message())));
                        }) as Box<dyn FnMut(_)>);
                        ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
                        onerror_callback.forget(); //leak the closure
                    }
                    Err(e) => {
                        c.send_if_needed(Err(Error::CantConnect(e.as_string().unwrap_or_else(|| "Unknown error".to_string()))));
                    }
                }
            }).unwrap();
        match f.await {
            Ok(_) => {
                logwise::info_sync!("WebsocketAdapter created successfully");
                Ok(WebsocketAdapter {})
            }
            Err(e) => {
                logwise::error_sync!("Failed to create WebsocketAdapter: {e}", e=logwise::privacy::LogIt(&e));
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