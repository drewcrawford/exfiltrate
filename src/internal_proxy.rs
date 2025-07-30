mod websocket_adapter;

use std::net::TcpStream;
use std::sync::{Arc, LazyLock};
use crate::internal_proxy::Error::NotConnected;
use crate::bidirectional_proxy::BidirectionalProxy;
use crate::spinlock::Spinlock;

#[derive(Debug)]
pub enum Error {
    NotConnected,
}

static INTERNAL_PROXY: LazyLock<InternalProxy> = LazyLock::new(|| {
    InternalProxy::new()
});

#[derive(Debug)]
enum ProxyState {
    NotConnected,
    // This state is used on wasm to indicate that we are trying to connect
    Connecting,
    Connected(BidirectionalProxy<Stream>),
}

#[cfg(not(target_arch = "wasm32"))]
type Stream = TcpStream;
#[cfg(target_arch = "wasm32")]
type Stream = websocket_adapter::WebsocketAdapter;

#[derive(Debug)]
pub struct InternalProxy {
    //in practice, notifications are sent from the main thread on wasm, so we can't use a simple Mutex
    buffered_notifications: Spinlock<Vec<crate::jrpc::Notification>>,
    //For similar reasons, let's use a dumb spinlock here.
    //Be careful with lock ordering; if you're going to hold both take this one first.
    bidirectional_proxy: Arc<Spinlock<ProxyState>>,
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
        let m = InternalProxy {
            buffered_notifications: Spinlock::new(Vec::new()),
            bidirectional_proxy: Arc::new(Spinlock::new(ProxyState::NotConnected)),
        };
        m.reconnect_if_possible();
        m
    }

    fn reconnect_if_possible(&self) {

        let _wasm_connecting = self.bidirectional_proxy.with_mut(|proxy| {
            match proxy {
                ProxyState::Connected(_) => {
                    //already connected
                    false
                }
                ProxyState::Connecting => {
                    //already connecting
                    false
                }
                ProxyState::NotConnected => {
                    #[cfg(not(target_arch = "wasm32"))] {
                        let s = TcpStream::connect(ADDR);
                        match s {
                            Ok(stream) => {
                                let stream = crate::bidirectional_proxy::BidirectionalProxy::new(stream, bidi_fn);
                                *proxy = ProxyState::Connected(stream);
                            }
                            Err(e) => {
                                eprintln!("ip: Failed to reconnect to {}: {}", ADDR, e);
                            }
                        }
                        false
                    }
                    #[cfg(target_arch = "wasm32")] {
                        *proxy = ProxyState::Connecting;
                        true
                    }
                }
            }
        });

        #[cfg(target_arch = "wasm32")]
        if _wasm_connecting {
            let move_proxy = self.bidirectional_proxy.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let stream = websocket_adapter::WebsocketAdapter::new().await;
                match stream {
                    Ok(stream) => {
                        let stream = crate::bidirectional_proxy::BidirectionalProxy::new(stream, bidi_fn);
                        move_proxy.with_mut(|proxy| {
                            *proxy = ProxyState::Connected(stream);
                        });
                    }
                    Err(e) => {
                        eprintln!("ip: Failed to reconnect to {}: {}", ADDR, e);
                    }
                }
            })
        }


    }

    pub fn send_notification(&self, notification: crate::jrpc::Notification) -> Result<(), Error> {
        self.send_buffered_if_possible();
        self.bidirectional_proxy.with_mut(|proxy| {
            match proxy {
                ProxyState::Connected(proxy) => {
                    let msg = serde_json::to_string(&notification).map_err(|_| NotConnected)?;
                    proxy.send(msg.as_bytes()).map_err(|_| NotConnected)
                }
                _ => Err(NotConnected),
            }
        })
    }
    pub fn buffer_notification(&self, notification: crate::jrpc::Notification) {
        self.buffered_notifications.with_mut(|n| {
            n.push(notification);
        });
        self.send_buffered_if_possible();
    }

    fn send_buffered_if_possible(&self) {
        self.reconnect_if_possible();
        self.bidirectional_proxy.with_mut(|proxy| {
            match proxy {
                ProxyState::Connected(proxy) => {
                    //send buffered notifications
                    //short lock
                    let buffered = self.buffered_notifications.with_mut(|n| n.drain(..).collect::<Vec<_>>());

                    for msg in buffered {
                        let msg = serde_json::to_string(&msg).unwrap();
                        proxy.send(msg.as_bytes()).unwrap();
                    }
                }
                _ => {
                    //not connected, do nothing
                }
            }
        });
    }

    pub fn current() -> &'static InternalProxy {
        &INTERNAL_PROXY
    }

}