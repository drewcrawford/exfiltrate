use std::sync::{Arc, LazyLock, Mutex};

static CURRENT_LOGPROXY: LazyLock<LogProxy> = LazyLock::new(|| {
    LogProxy::new()
});

pub struct LogProxy {
    logs: Arc<Mutex<Vec<String>>>,
}

impl LogProxy {
    pub fn current() -> &'static LogProxy {
        &CURRENT_LOGPROXY
    }
    fn new() -> LogProxy {
        LogProxy{
            logs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn reset(&self) {
        self.logs.lock().unwrap().clear();
    }
    pub fn add_log(&self, log: String) {
        self.logs.lock().unwrap().push(log);
    }
}