mod websocket_adapter;

use std::net::TcpStream;
use std::sync::{LazyLock};
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

#[cfg(not(target_arch = "wasm32"))]
type Stream = TcpStream;
#[cfg(target_arch = "wasm32")]
type Stream = websocket_adapter::WebsocketAdapter;

#[derive(Debug)]
struct Mut {
    bidirectional_proxy: Option<BidirectionalProxy<Stream>>,
    buffered_notifications: Vec<crate::jrpc::Notification>,
}

#[derive(Debug)]
pub struct InternalProxy {
    //in practice, notifications are sent from the main thread on wasm, so we can't use a simple Mutex
    m: crate::spinlock::Spinlock<Mut>,
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
        #[cfg(not(target_arch = "wasm32"))]
        let connect = std::net::TcpStream::connect(ADDR);
        #[cfg(target_arch = "wasm32")]
        let connect = websocket_adapter::WebsocketAdapter::new();

        let bidirectional = connect.ok().map(|stream| crate::bidirectional_proxy::BidirectionalProxy::new(stream, bidi_fn));
        let m = Mut {
            bidirectional_proxy: bidirectional,
            buffered_notifications: Vec::new(),
        };


        InternalProxy {
            m: Spinlock::new(m),
        }
    }

    fn reconnect_if_possible(m: &mut Mut) {
        if m.bidirectional_proxy.is_none() {
            #[cfg(not(target_arch = "wasm32"))]
            let s = TcpStream::connect(ADDR);
            #[cfg(target_arch = "wasm32")]
            let s = websocket_adapter::WebsocketAdapter::new();
            match s {
                Ok(stream) => {
                    m.bidirectional_proxy = Some(crate::bidirectional_proxy::BidirectionalProxy::new(stream, bidi_fn));
                }
                Err(e) => {
                    eprintln!("ip: Failed to reconnect to {}: {}", ADDR, e);
                }
            }
        }
    }



    pub fn send_notification(&self, notification: crate::jrpc::Notification) -> Result<(), Error> {
        self.m.with_mut(|m| {
            //todo: this critical section is a bit long for a spinlock.
            Self::send_buffered_if_possible(m);
            match m.bidirectional_proxy.as_mut() {
                Some(proxy) => {
                    let msg = serde_json::to_string(&notification).map_err(|_| NotConnected)?;
                    proxy.send(msg.as_bytes()).map_err(|_| NotConnected)
                }
                None => Err(NotConnected),
            }
        })
    }
    pub fn buffer_notification(&self, notification: crate::jrpc::Notification) {
        self.m.with_mut(|m| {
            m.buffered_notifications.push(notification);
            Self::send_buffered_if_possible(m);
        });
    }

    fn send_buffered_if_possible(m: &mut Mut) {
        Self::reconnect_if_possible(m);
        match m.bidirectional_proxy.as_mut() {
            Some(proxy) => {
                for msg in m.buffered_notifications.drain(..) {
                    let msg = serde_json::to_string(&msg).unwrap();
                    proxy.send(msg.as_bytes()).unwrap();
                }
            }
            None => {
                //don't send
            }
        }
    }

    pub fn current() -> &'static InternalProxy {
        &INTERNAL_PROXY
    }

}