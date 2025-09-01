#[cfg(not(target_arch = "wasm32"))]
use std::time;
#[cfg(target_arch = "wasm32")]
use web_time as time;

pub fn main() {
    logwise::info_sync!("LOG MESSAGE 0");
    exfiltrate::logwise::begin_capture();
    logwise::info_sync!("LOG MESSAGE 1");
    logwise::info_sync!("LOG MESSAGE 2");
    #[cfg(not(target_arch = "wasm32"))]
    //on wasm this is illegal and unnecessary
    std::thread::sleep(time::Duration::from_secs(1_000));
}
