#[cfg(target_arch = "wasm32")]
pub use web_time as time;
#[cfg(not(target_arch = "wasm32"))]
pub use std::time;