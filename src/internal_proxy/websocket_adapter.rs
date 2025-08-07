#![cfg(target_arch = "wasm32")]

use std::fmt::Display;
use std::time::Duration;
use wasm_bindgen::JsCast;
use std::sync::{Arc, Mutex};
use super::super::logging::log;

use wasm_bindgen::closure::Closure;
use crate::bidirectional_proxy::{ReadTransport, WriteTransport};


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

const ADDR: &str = "ws://localhost:1984";

#[derive(Debug)]
pub struct WriteAdapter {
    send: continue_stream::Sender<Vec<u8>>,
}
#[derive(Debug)]
pub struct ReadApapter {
    recv: std::sync::mpsc::Receiver<Vec<u8>>,
}

pub async fn adapter() -> Result<(WriteAdapter, ReadApapter), Error> {
    //put ws communication on its own thread
    let (c,f) = r#continue::continuation();
    let (read_send,read_recv) = std::sync::mpsc::channel();
    let (send_send,send_recv) = continue_stream::continuation();

    crate::sys::thread::Builder::new()
        .name("exfiltrate::WebsocketAdapter".to_owned())
        .spawn(move || {
            log("thread started");
            let ws = web_sys::WebSocket::new(ADDR);
            log("WebSocket created");
            let c = OneShot::new(c);
            match ws {
                Ok(ws) => {
                    ws.set_binary_type(web_sys::BinaryType::Arraybuffer);
                    let move_c = c.clone();
                    let onopen_callback = Closure::wrap(Box::new(move |event: web_sys::Event| {
                        web_sys::console::log_1(&"WebSocket opened!".into());
                        move_c.send_if_needed(Ok(()));
                    }) as Box<dyn FnMut(_)>);
                    ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
                    onopen_callback.forget(); //leak the closure

                    let move_c = c.clone();
                    let onerror_callback = Closure::wrap(Box::new(move |event: web_sys::ErrorEvent| {
                        // .message seems problematic in some cases?
                        let error_description = event.type_();
                        let error_msg = format!("Websocket error: {}", error_description);
                        web_sys::console::log_1(&error_msg.into());
                        move_c.send_if_needed(Err(Error::CantConnect(error_description)));
                    }) as Box<dyn FnMut(_)>);
                    ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
                    onerror_callback.forget(); //leak the closure

                    let onclose_callback = Closure::wrap(Box::new(move |event: web_sys::CloseEvent| {
                        web_sys::console::log_1(&"WebSocket closed!".into());
                    }) as Box<dyn FnMut(_)>);
                    ws.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));
                    onclose_callback.forget(); //leak the closure
                    let onmessage_callback = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
                        if let Ok(abuf) = event.data().dyn_into::<web_sys::js_sys::ArrayBuffer>() {
                            let u8_array = web_sys::js_sys::Uint8Array::new(&abuf);
                            let mut vec = vec![0; u8_array.length() as usize];
                            u8_array.copy_to(&mut vec[..]);
                            read_send.send(vec);
                        }
                        else {
                            let str = format!("Received non-binary message: {:?}", event.data());
                            web_sys::console::log_1(&str.into());
                            todo!()
                        }
                        return;
                    }) as Box<dyn FnMut(_)>);
                    ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
                    onmessage_callback.forget(); //leak the closure

                    //set up an async task to read from the stream
                    wasm_bindgen_futures::spawn_local(async move {
                        loop {
                            let msg: Option<Vec<u8>> = send_recv.receive().await;
                            // web_sys::console::log_1(&"WebSocketAdapter: will send data".into());
                            if msg.is_none() {
                                web_sys::console::log_1(&"WebSocketAdapter: send_recv closed".into());
                                break;
                            }
                            let msg = msg.unwrap();
                            //can't use send_with_u8_array, see https://github.com/wasm-bindgen/wasm-bindgen/issues/4101
                            let len = msg.len();
                            let msg = web_sys::js_sys::Uint8Array::from(msg.as_slice());
                            let msg = msg.buffer();
                            let op = ws.send_with_array_buffer(&msg);
                            match op {
                                Ok(_) => {
                                    // web_sys::console::log_1(&format!("WebSocketAdapter: sent {} bytes", len).into());
                                }
                                Err(e) => {
                                    web_sys::console::error_1(&format!("WebSocketAdapter: failed to send data: {:?}", e).into());
                                    break;
                                }
                            }
                        }
                    });


                }
                Err(e) => {
                    c.send_if_needed(Err(Error::CantConnect(e.as_string().unwrap_or_else(|| "Unknown error".to_string()))));
                }
            }
            patch_close();

        }).unwrap();

    match f.await {
        Ok(_) => {
            log("WebsocketAdapter created successfully");
            Ok((WriteAdapter { send: send_send }, ReadApapter { recv: read_recv }))
        }
        Err(e) => {
            log(&format!("WebsocketAdapter creation failed: {:?}", e));
            Err(e)
        }
    }
}

pub fn patch_close() {
    //forbid thread exit
    let global = web_sys::js_sys::global();
    let wrapper = Closure::wrap(Box::new(move || {
        web_sys::console::log_1(&"thread close called".into());
    }) as Box<dyn Fn()>);

    web_sys::js_sys::Reflect::set(&global, &"close".into(), wrapper.as_ref().unchecked_ref())
        .expect("failed to patch close");
    wrapper.forget();
}



impl WriteTransport for WriteAdapter {
    fn write(&mut self, data: &[u8]) -> Result<(), crate::bidirectional_proxy::Error> {
        // web_sys::console::log_1(&format!("WebsocketAdapter::write_block: sending {} bytes", data.len()).into());
        self.send.send(data.to_vec());
        Ok(())
    }

    fn flush(&mut self) -> Result<(), crate::bidirectional_proxy::Error> {
        //nothing to do!
        Ok(())
    }
}
impl ReadTransport for ReadApapter {
    fn read_nonblock(&mut self, buf: &mut [u8]) -> Result<usize, crate::bidirectional_proxy::Error> {
        match self.recv.try_recv() {
            Ok(data) => {
                if data.len() > buf.len() {
                    log("WebsocketAdapter::read_nonblock: buffer too small");
                    todo!()
                } else {
                    buf[..data.len()].copy_from_slice(&data);
                    Ok(data.len())
                }
            }
            Err(_) => Ok(0)
        }
    }
}