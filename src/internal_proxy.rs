mod websocket_adapter;

use std::cell::OnceCell;
use std::net::TcpStream;
use std::sync::{Arc, LazyLock, Mutex, Once};
use std::sync::atomic::AtomicBool;
use crate::internal_proxy::Error::NotConnected;
use crate::bidirectional_proxy::BidirectionalProxy;
use crate::once_nonlock::OnceNonLock;

#[derive(Debug)]
pub enum Error {
    NotConnected,
}

static INTERNAL_PROXY: LazyLock<InternalProxy> = LazyLock::new(|| {
    InternalProxy::new()
});


#[cfg(not(target_arch = "wasm32"))]
type Stream = TcpStream;
#[cfg(target_arch = "wasm32")]
type Stream = websocket_adapter::WebsocketAdapter;
use crate::spinlock::Spinlock;

#[derive(Debug)]
pub struct InternalProxy {
    //in practice, notifications are sent from the main thread on wasm, so we can't use a simple Mutex
    buffered_notification_sender: std::sync::mpsc::Sender<crate::jrpc::Notification>,
    //here we need mutex but we can simply fail if the lock is contended
    buffered_notification_receiver: Mutex<std::sync::mpsc::Receiver<crate::jrpc::Notification>>,

    bidirectional_proxy: Arc<OnceNonLock<BidirectionalProxy<Stream>>>,
}

fn bidi_fn(msg: Box<[u8]>) -> Option<Box<[u8]>> {
    //attempt parse as request
    eprintln!("ip: received bidi message: {:?}", String::from_utf8_lossy(&msg));
    let request: Result<crate::jrpc::Request, _> = serde_json::from_slice(&msg);
    match request {
        Ok(request) => {
            eprintln!("ip: received request: {:?}", request);
            let response = crate::mcp::dispatch_in_target(request);
            let response_bytes = serde_json::to_vec(&response).unwrap();
            eprintln!("ip: sending response {:?}", String::from_utf8_lossy(&response_bytes));
            Some(response_bytes.into_boxed_slice())
        }
        Err(e) => {
            todo!("Not implemented yet: Received request from internal proxy: {:?}", e);
        }
    }
}

const ADDR: &str = "127.0.0.1:1985";
impl InternalProxy {
    fn new() -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        let m = InternalProxy {
            buffered_notification_sender: sender,
            buffered_notification_receiver: Mutex::new(receiver),
            bidirectional_proxy: Arc::new(OnceNonLock::new()),
        };
        m.reconnect_if_possible();
        m
    }

    fn reconnect_if_possible(&self) {
        #[cfg(not(target_arch = "wasm32"))]
        self.bidirectional_proxy.try_get_or_init(|| {
            let s = TcpStream::connect(ADDR);
            match s {
                Ok(stream) => {
                    let stream = crate::bidirectional_proxy::BidirectionalProxy::new(stream, bidi_fn);
                    stream
                }
                Err(e) => {
                    panic!("Failed to reconnect to {}: {}", ADDR, e);
                }
            }
        });
        #[cfg(target_arch = "wasm32")] {
            //on wasm, we need to connect asynchronously
            let f = self.bidirectional_proxy.init_async(async move || {
                if web_sys::window().is_none() {
                    web_sys::console::error_1(&"WebsocketAdapter: No window available".into());
                    todo!("Needs thread persist trick?");
                }
                let stream = websocket_adapter::WebsocketAdapter::new().await;
                match stream {
                    Ok(stream) => {
                        let stream = crate::bidirectional_proxy::BidirectionalProxy::new(stream, bidi_fn);
                        stream
                    }
                    Err(e) => {
                        panic!("Failed to reconnect to {}: {}", ADDR, e);
                    }
                }
            });
            wasm_bindgen_futures::spawn_local(f)
        }

    }

    pub fn send_notification(&self, notification: crate::jrpc::Notification) -> Result<(), Error> {
        self.send_buffered_if_possible();
        if let Some(proxy) = self.bidirectional_proxy.get() {
            let msg = serde_json::to_string(&notification).map_err(|_| NotConnected)?;
            proxy.send(msg.as_bytes()).map_err(|_| NotConnected)
        } else {
            //not connected
            Err(NotConnected)
        }
    }
    pub fn buffer_notification(&self, notification: crate::jrpc::Notification) {
        self.buffered_notification_sender.send(notification).unwrap();
        self.send_buffered_if_possible();
    }

    fn send_buffered_if_possible(&self) {
        self.reconnect_if_possible();
        if let Some(proxy) = self.bidirectional_proxy.get() {
            //short lock
            let mut take = Vec::new();
            if let Some(buffered_receiver) = self.buffered_notification_receiver.try_lock().ok() {
                while let Some(notification) = buffered_receiver.try_recv().ok() {
                    take.push(notification);
                }
            }
            else {
                crate::logging::log(&"ip: Send contended");

            }
            for notification in take {
                let msg = serde_json::to_string(&notification).unwrap();
                if let Err(e) = proxy.send(msg.as_bytes()) {
                    crate::logging::log(&format!("ip: Failed to send buffered notification: {}", e));
                }
            }
        }
    }

    pub fn current() -> &'static InternalProxy {
        &INTERNAL_PROXY
    }

}