#[cfg(target_arch = "wasm32")]
pub use web_time as time;
#[cfg(not(target_arch = "wasm32"))]
pub use std::time;

#[cfg(target_arch = "wasm32")]
pub use wasm_thread as thread;

#[cfg(not(target_arch = "wasm32"))]
pub use std::thread;