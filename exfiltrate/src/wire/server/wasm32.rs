use crate::wire::server::do_command;
use exfiltrate_internal::rpc::RPC;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use wasm_bindgen_futures::JsFuture;
use wasm_safe_mutex::Mutex;
use web_sys::js_sys::global;
use web_sys::wasm_bindgen::JsCast;
use web_sys::{Request, RequestInit, RequestMode, Response, WorkerGlobalScope};

pub fn wasm32_go() {
    patch_close();
    let thread_result = wasm_thread::Builder::new()
        .name("exfiltrate::wasm".to_string())
        .spawn(|| {
            patch_close();
            wasm_bindgen_futures::spawn_local(async move {
                let receiver = SEND_WORKER_MESSAGE
                    .1
                    .with_mut_sync(|e| e.take())
                    .expect("no receiver");
                worker_thread(receiver).await;
            });
        });
    match thread_result {
        Ok(_join_handle) => {}
        Err(e) => {
            web_sys::console::error_1(&JsValue::from_str(&format!("{:?}", e)));
            panic!("{:?}", e);
        }
    }
}

fn handle_msg(data: &[u8]) -> Result<RPC, String> {
    //parse as RPC
    web_sys::console::log_1(&"handle_msg: A".into());
    match rmp_serde::from_slice(data) {
        Ok(msg) => {
            web_sys::console::log_1(&"handle_msg: B".into());

            let msg: RPC = msg;
            match msg {
                RPC::Command(command) => {
                    let reply = do_command(command);
                    web_sys::console::log_1(&"handle_msg: C".into());

                    Ok(RPC::CommandResponse(reply))
                }
                RPC::CommandResponse(r) => Err(format!("Expected command, got: {:?}", r)),
                _ => Err("Unknown RPC variant received".to_string()),
            }
        }
        Err(e) => Err(format!("{:?}", e)),
    }
}

/// Debug a WebSocket URL by hitting it over HTTP from a *worker*.
pub async fn debug_ws_handshake_in_worker(ws_url: &str) -> Result<(), JsValue> {
    // globalThis inside a worker is a WorkerGlobalScope

    let global = global();
    let scope: WorkerGlobalScope = global
        .dyn_into()
        .expect("global() should be a WorkerGlobalScope in a worker");

    // Convert ws:// -> http:// and wss:// -> https://
    let http_url = if ws_url.starts_with("ws://") {
        ws_url.replacen("ws://", "http://", 1)
    } else if ws_url.starts_with("wss://") {
        ws_url.replacen("wss://", "https://", 1)
    } else {
        ws_url.to_string()
    };

    let opts = RequestInit::new();
    opts.set_method("GET");
    // CORS mode is usually fine; tweak if you know you're same-origin
    opts.set_mode(RequestMode::Cors);

    let request = Request::new_with_str_and_init(&http_url, &opts)?;
    // NOTE: call fetch on the WorkerGlobalScope, not window()
    let resp_value = JsFuture::from(scope.fetch_with_request(&request)).await;
    match resp_value {
        Err(err) => {
            web_sys::console::log_1(&format!("fetch error: {:?}", err).into());
        }
        Ok(resp_value) => {
            let resp: Response = resp_value.dyn_into()?;
            web_sys::console::log_1(&"got response".into());

            web_sys::console::log_1(&format!("Fetch status: {}", resp.status()).into());

            let body_js = JsFuture::from(resp.text()?).await?;
            web_sys::console::log_1(&body_js);
        }
    }
    Ok(())
}

/// Main worker thread function that manages WebSocket connections.
///
/// This function runs in a dedicated thread and:
/// - Handles connection requests
/// - Manages the WebSocket lifecycle
/// - Routes messages between the WebSocket and the proxy system
///
/// # Arguments
///
/// * `receiver` - Channel for receiving control messages
async fn worker_thread(receiver: continue_stream::Receiver<WorkerMessage>) {
    web_sys::console::log_1(&"thread started".into());

    let mut socket = None;
    SEND_WORKER_MESSAGE.0.send(WorkerMessage::Reconnect);

    loop {
        let r = receiver.receive().await;

        match r {
            Some(WorkerMessage::Reconnect) => {
                match &socket {
                    None => {
                        web_sys::console::log_1(&"WebSocket: connecting...".into());

                        let s = create_web_socket().await;
                        match s {
                            Ok(_) => {
                                web_sys::console::log_1(&" WebSocket created successfully".into());
                                socket = Some(s);
                            }
                            Err(e) => {
                                web_sys::console::log_1(
                                    &format!("Failed to create WebSocket: {:?}", e).into(),
                                );
                            }
                        }
                    }
                    Some(..) => {
                        //we already have a socket so nothing to do I guess?
                    }
                }
            }
            None => {
                web_sys::console::log_1(&"receiver closed, exiting thread".into());
                break;
            }
        }
    }
}

fn patch_close() {
    //forbid thread exit
    let global = web_sys::js_sys::global();
    let wrapper = Closure::wrap(Box::new(move || {
        web_sys::console::log_1(&"thread close called".into());
    }) as Box<dyn Fn()>);

    web_sys::js_sys::Reflect::set(&global, &"close".into(), wrapper.as_ref().unchecked_ref())
        .expect("failed to patch close");
    wrapper.forget();
}
enum WorkerMessage {
    Reconnect,
}

#[allow(clippy::type_complexity)]
static SEND_WORKER_MESSAGE: LazyLock<(
    continue_stream::Sender<WorkerMessage>,
    Mutex<Option<continue_stream::Receiver<WorkerMessage>>>,
)> = LazyLock::new(|| {
    let (s, r) = continue_stream::continuation();
    (s, Mutex::new(Some(r)))
});

/// A one-shot sender that can only send a value once.
///
/// This is used for sending completion signals from WebSocket
/// event handlers back to the async context. It ensures that
/// only the first event (either success or error) is processed.
struct OneShot<T> {
    c: Arc<Mutex<Option<r#continue::Sender<T>>>>,
}

impl<T> OneShot<T> {
    /// Creates a new one-shot sender.
    fn new(sender: r#continue::Sender<T>) -> Self {
        OneShot {
            c: Arc::new(Mutex::new(Some(sender))),
        }
    }

    /// Sends a value if not already sent.
    ///
    /// This method is idempotent - subsequent calls after the first
    /// successful send will be no-ops.
    fn send_if_needed(&self, value: T) {
        if let Some(sender) = self.c.with_mut_sync(|l| l.take()) {
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

const WEB_ADDR: &str = "ws://localhost:1338";

async fn create_web_socket() -> Result<web_sys::WebSocket, String> {
    let ws = web_sys::WebSocket::new(WEB_ADDR);
    match ws {
        Ok(ws) => {
            let (func_sender, func_fut) = r#continue::continuation::<Result<(), String>>();
            let func_sender = OneShot::new(func_sender);
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
                // .message seems problematic here?
                //let's try fetch API
                wasm_bindgen_futures::spawn_local(async move {
                    debug_ws_handshake_in_worker(WEB_ADDR).await.unwrap();
                });
                web_sys::console::log_1(&"Websocket error:".into());
                web_sys::console::log_1(&event);
                move_func_sender.send_if_needed(Err("Cannot connect to server".to_string()));
            }) as Box<dyn FnMut(_)>);
            ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
            onerror_callback.forget(); //leak the closure

            let onclose_callback = Closure::wrap(Box::new(move |_event: web_sys::CloseEvent| {
                web_sys::console::log_1(&"WebSocket closed:".into());
                web_sys::console::log_1(&_event);
                wasm_thread::sleep(Duration::from_secs(10));
                SEND_WORKER_MESSAGE.0.send(WorkerMessage::Reconnect);
            }) as Box<dyn FnMut(_)>);
            ws.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));
            onclose_callback.forget(); //leak the closure
            let move_ws = ws.clone();
            let onmessage_callback = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
                web_sys::console::log_1(&"A0".into());
                if let Ok(abuf) = event.data().dyn_into::<web_sys::js_sys::ArrayBuffer>() {
                    let u8_array = web_sys::js_sys::Uint8Array::new(&abuf);
                    let mut vec = vec![0; u8_array.length() as usize];
                    u8_array.copy_to(&mut vec[..]);
                    web_sys::console::log_1(&"A".into());

                    let reply_rpc = handle_msg(&vec);
                    web_sys::console::log_1(&"B".into());

                    match reply_rpc {
                        Ok(mut rpc) => {
                            let mut attachments = Vec::new();
                            if let RPC::CommandResponse(ref mut resp) = rpc {
                                attachments = resp.response.split_data();
                                resp.num_attachments = attachments.len() as u32;
                            }

                            // Serialize the main message (now small)
                            let msgpack_reply = rmp_serde::to_vec(&rpc).unwrap();
                            let msg = web_sys::js_sys::Uint8Array::from(msgpack_reply.as_slice());
                            let msg = msg.buffer();
                            move_ws.send_with_array_buffer(&msg).unwrap();

                            // Send attachments
                            for attachment in attachments {
                                let msg = web_sys::js_sys::Uint8Array::from(attachment.as_slice());
                                let msg = msg.buffer();
                                move_ws.send_with_array_buffer(&msg).unwrap();
                            }
                            web_sys::console::log_1(&"D".into());
                        }
                        Err(e) => {
                            web_sys::console::error_1(
                                &format!("Error handling message: {}", e).into(),
                            );
                        }
                    }
                } else {
                    let str = format!("Received non-binary message: {:?}", event.data());
                    web_sys::console::log_1(&str.into());
                    unimplemented!("This is not currently supported");
                }
            }) as Box<dyn FnMut(_)>);
            ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
            onmessage_callback.forget(); //leak the closure

            let f = func_fut.await;
            f.map(|_| ws)
        }
        Err(e) => Err(format!("{:?}", e)),
    }
}
