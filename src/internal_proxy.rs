use std::net::TcpStream;
use std::sync::{Arc, LazyLock, Mutex};
use crate::internal_proxy::Error::NotConnected;
use std::io::Write;
use crate::bidirectional_proxy::BidirectionalProxy;
use crate::jrpc::Request;

#[derive(Debug)]
pub enum Error {
    NotConnected,
}

static INTERNAL_PROXY: LazyLock<InternalProxy> = LazyLock::new(|| {
    InternalProxy::new()
});

#[derive(Debug)]
struct Mut {
    bidirectional_proxy: Option<BidirectionalProxy<TcpStream>>
}

#[derive(Debug)]
pub struct InternalProxy {
    m: Mutex<Mut>,
}

const ADDR: &str = "127.0.0.1:1985";
impl InternalProxy {
    fn new() -> Self {
        let connect = std::net::TcpStream::connect(ADDR);
        let bidirectional = connect.ok().map(|stream| crate::bidirectional_proxy::BidirectionalProxy::new(stream, |msg| {
            //attempt parse as request
            let request: Result<crate::jrpc::Request, _> = serde_json::from_slice(&msg);
            match request {
                Ok(request) => {
                    eprintln!("Received request from internal proxy: {:?}", request);
                    let response = crate::mcp::dispatch(request);
                    let response_bytes = serde_json::to_vec(&response).unwrap();
                    eprintln!("Sending response from internal proxy {:?}", String::from_utf8_lossy(&response_bytes));
                    Some(response_bytes.into_boxed_slice())
                }
                Err(e) => {
                    todo!("Not implemented yet: Received request from internal proxy: {:?}", e);
                }
            }
        }));


        let m = Mut {
            bidirectional_proxy: bidirectional,
        };

        InternalProxy {
            m: Mutex::new(m),
        }
    }

    fn reconnect_if_possible(m: &mut Mut) {
        if m.bidirectional_proxy.is_none() {
            let s = TcpStream::connect(ADDR);
            match s {
                Ok(stream) => {
                    m.bidirectional_proxy = Some(crate::bidirectional_proxy::BidirectionalProxy::new(stream, |msg| {
                        todo!();
                    }));
                }
                Err(e) => {
                    eprintln!("Failed to reconnect to {}: {}", ADDR, e);
                }
            }
        }
    }



    pub fn send_notification(&self, notification: crate::jrpc::Notification) -> Result<(), Error> {
        let mut lock = self.m.lock().unwrap();
        Self::reconnect_if_possible(&mut *lock);
        match lock.bidirectional_proxy.as_mut() {
            Some(proxy) => {
                let msg = serde_json::to_string(&notification).map_err(|_| NotConnected)?;
                proxy.send(msg.as_bytes()).map_err(|_| NotConnected)
            }
            None => Err(NotConnected),
        }

    }

    pub fn current() -> &'static InternalProxy {
        &INTERNAL_PROXY
    }

}