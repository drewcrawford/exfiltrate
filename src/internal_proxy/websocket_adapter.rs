#![cfg(target_arch = "wasm32")]

use std::fmt::Display;
use wasm_bindgen::JsCast;
use std::sync::{Arc, Mutex};
use super::super::logging::log;

use wasm_bindgen::closure::Closure;
use crate::bidirectional_proxy::{ReadTransport, WriteTransport};
use crate::once_nonlock::OnceNonLock;

#[derive(Debug)]
pub enum Error {
    #[allow(dead_code)]
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
    buf: Vec<u8>,
}

static SEND_WORKER_MESSAGE: OnceNonLock<continue_stream::Sender<WorkerMessage>> = OnceNonLock::new();


struct ReconnectMessage{
    func_sender: r#continue::Sender<Result<(WriteAdapter, ReadApapter), Error>>,
}
struct SocketClosedMessage;

enum WorkerMessage {
    Reconnect(ReconnectMessage),
    SocketClosed(SocketClosedMessage),
}

async fn worker_thread(receiver: continue_stream::Receiver<WorkerMessage>) {
    log("thread started");


    let mut socket = None;

    loop {
        let r = receiver.receive().await;
        match r {
            Some(WorkerMessage::Reconnect(reconnect)) => {
                match &socket {
                    None => {
                        log("WebSocketAdapter: received reconnect message");
                        let (write_send, write_recv) = continue_stream::continuation::<Vec<u8>>();
                        let (read_send, read_recv) = std::sync::mpsc::channel::<Vec<u8>>();

                        let s = create_web_socket(read_send, write_recv).await;
                        match s {
                            Ok(_) => {
                                log("WebSocketAdapter: WebSocket created successfully");
                                socket = Some(s);
                                reconnect.func_sender.send(Ok((
                                    WriteAdapter {
                                        send: write_send,
                                    },
                                    ReadApapter {
                                        recv: read_recv,
                                        buf: Vec::new(),
                                    },
                                )));
                            }
                            Err(e) => {
                                log(&format!("WebSocketAdapter: Failed to create WebSocket: {:?}", e));
                                reconnect.func_sender.send(Err(e));
                                // Optionally, you could send an error back to the main thread here
                            }
                        }
                    }
                    Some(socket) => {
                        //we already have a socket so nothing to do I guess?
                    }
                }
            }
            Some(WorkerMessage::SocketClosed(SocketClosedMessage)) => {
                log("WebSocketAdapter: received socket closed message");
                // Handle socket closed message if needed
                socket = None; // Reset the socket
            }
            None => {
                log("WebSocketAdapter: receiver closed, exiting thread");
                break;
            }
        }
    }
}

async fn create_web_socket(read_send: std::sync::mpsc::Sender<Vec<u8>>, write_recv: continue_stream::Receiver<Vec<u8>>) -> Result<web_sys::WebSocket, Error> {
    let ws = web_sys::WebSocket::new(ADDR);
    log("WebSocket created");
    let (func_sender,func_fut) = r#continue::continuation::<Result<(), Error>>();
    let func_sender = OneShot::new(func_sender);
    match ws {
        Ok(ws) => {
            ws.set_binary_type(web_sys::BinaryType::Arraybuffer);
            let move_func_sender = func_sender.clone();
            let onopen_callback = Closure::wrap(Box::new(move |_event: web_sys::Event| {
                web_sys::console::log_1(&"WebSocket opened!".into());
                move_func_sender.send_if_needed(Ok(()));
            }) as Box<dyn FnMut(_)>);
            ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
            onopen_callback.forget(); //leak the closure

            let move_func_sender = func_sender.clone();
            let onerror_callback = Closure::wrap(Box::new(move |event: web_sys::ErrorEvent| {
                // .message seems problematic in some cases?
                let error_description = event.type_();
                let error_msg = format!("Websocket error: {}", error_description);
                web_sys::console::log_1(&error_msg.into());
                move_func_sender.send_if_needed(Err(Error::CantConnect(error_description)));
            }) as Box<dyn FnMut(_)>);
            ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
            onerror_callback.forget(); //leak the closure

            let onclose_callback = Closure::wrap(Box::new(move |_event: web_sys::CloseEvent| {
                web_sys::console::log_1(&"WebSocket closed!".into());
                SEND_WORKER_MESSAGE.get().as_ref().map(|sender| {
                    sender.send(WorkerMessage::SocketClosed(SocketClosedMessage));
                });
            }) as Box<dyn FnMut(_)>);
            ws.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));
            onclose_callback.forget(); //leak the closure
            let onmessage_callback = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
                if let Ok(abuf) = event.data().dyn_into::<web_sys::js_sys::ArrayBuffer>() {
                    let u8_array = web_sys::js_sys::Uint8Array::new(&abuf);
                    let mut vec = vec![0; u8_array.length() as usize];
                    u8_array.copy_to(&mut vec[..]);
                    read_send.send(vec).unwrap();
                }
                else {
                    let str = format!("Received non-binary message: {:?}", event.data());
                    web_sys::console::log_1(&str.into());
                    unimplemented!("This is not currently supported");
                }
                return;
            }) as Box<dyn FnMut(_)>);
            ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
            onmessage_callback.forget(); //leak the closure

            //set up an async task to read from the stream / send to the websocket
            let move_socket = ws.clone();
            patch_close();
            wasm_bindgen_futures::spawn_local(async move {
                loop {
                    let msg: Option<Vec<u8>> = write_recv.receive().await;
                    // web_sys::console::log_1(&"WebSocketAdapter: will send data".into());
                    if msg.is_none() {
                        web_sys::console::log_1(&"WebSocketAdapter: send_recv closed".into());
                        break;
                    }
                    let msg = msg.unwrap();
                    //can't use send_with_u8_array, see https://github.com/wasm-bindgen/wasm-bindgen/issues/4101
                    let msg = web_sys::js_sys::Uint8Array::from(msg.as_slice());
                    let msg = msg.buffer();
                    let op = move_socket.send_with_array_buffer(&msg);
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
            let f = func_fut.await;
            f.map(|_| ws)

        }
        Err(e) => {
            Err(Error::CantConnect(e.as_string().unwrap_or_else(|| "Unknown error".to_string())))
        }
    }

}

pub async fn adapter() -> Result<(WriteAdapter, ReadApapter), Error> {
    //put ws communication on its own thread
    //one thread only per process!
    SEND_WORKER_MESSAGE.try_get_or_init(move || {
        let (c,r) = continue_stream::continuation();
        crate::sys::thread::Builder::new()
            .name("exfiltrate::WebsocketAdapterWorker".to_owned())
            .spawn(|| {
                patch_close();
                wasm_bindgen_futures::spawn_local(worker_thread(r))
            })
            .expect("Failed to spawn WebsocketAdapter worker thread");
        Some(c)
    });
    match SEND_WORKER_MESSAGE.get().as_ref() {
        Some(sender) => {
            let (func_send, func_recv) = r#continue::continuation::<Result<(WriteAdapter, ReadApapter), Error>>();
            //send a reconnect message to the worker thread
            sender.send(WorkerMessage::Reconnect(ReconnectMessage{
                func_sender: func_send,
            }));
            func_recv.await
        }
        None => {
            log("WebsocketAdapter: worker thread not initialized");
            Err(Error::CantConnect("Worker thread not initialized".to_string()))
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
        //copy from self.buf first
        if !self.buf.is_empty() {
            let copy_bytes = std::cmp::min(self.buf.len(), buf.len());
            buf[..copy_bytes].copy_from_slice(&self.buf[..copy_bytes]);
            self.buf.drain(..copy_bytes);
            return Ok(copy_bytes);
        }
        match self.recv.try_recv() {
            Ok(data) => {
                //copy the first part into buf
                let copy_bytes = std::cmp::min(data.len(), buf.len());
                buf[..copy_bytes].copy_from_slice(&data[..copy_bytes]);
                //if there are more bytes, put them in self.buf
                if data.len() > copy_bytes {
                    self.buf.extend_from_slice(&data[copy_bytes..]);
                }
                Ok(copy_bytes)
            }
            Err(_) => Ok(0)
        }
    }
}