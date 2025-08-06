pub fn log(str: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        eprintln!("{}", str);
    }
    #[cfg(target_arch = "wasm32")]
    {
        use web_sys::console;
        console::log_1(&str.into());
    }
}
