//! Logwise integration for log capture and retrieval.
//!
//! This module provides integration with the `logwise` logging framework, allowing
//! exfiltrate to capture and retrieve log records from the running application.

use exfiltrate_internal::command::{Command, FileInfo, Response};
use logwise::LogRecord;
use std::pin::Pin;
use std::sync::{Arc, LazyLock};
use wasm_safe_mutex::Mutex;

#[derive(Debug)]
struct ExfiltrateLogger {
    records: Mutex<Vec<LogRecord>>,
}

impl ExfiltrateLogger {
    const fn new() -> ExfiltrateLogger {
        ExfiltrateLogger {
            records: Mutex::new(Vec::new()),
        }
    }
}

static LOGGER: LazyLock<Arc<ExfiltrateLogger>> =
    LazyLock::new(|| Arc::new(ExfiltrateLogger::new()));

impl logwise::Logger for ExfiltrateLogger {
    fn finish_log_record(&self, record: LogRecord) {
        self.records.with_mut_sync(|e| e.push(record));
    }

    fn finish_log_record_async<'s>(
        &'s self,
        record: LogRecord,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 's>> {
        Box::pin(self.records.with_mut_async(|e| e.push(record)))
    }

    fn prepare_to_die(&self) {}
}

/// Starts capturing logs from the `logwise` crate.
///
/// Adds a global logger that stores log records in memory.
/// These logs can then be retrieved via the `logwise_logs` command.
pub fn begin_log_capture() {
    logwise::add_global_logger(LOGGER.clone());
    crate::add_command(LogwiseCapture);
}

/// The `logwise_logs` command.
///
/// Retrieves all captured log records.
pub struct LogwiseCapture;

impl Command for LogwiseCapture {
    fn name(&self) -> &'static str {
        "logwise_logs"
    }

    fn short_description(&self) -> &'static str {
        "Shows logwise logs.  Use this to stream logs from a running Rust program.  ALWAYS use this to read
        logs on wasm32-unknown-unknown, since other methods are broken."
    }

    fn full_description(&self) -> &'static str {
        "Shows logwise logs.

In some cases, logs may be difficult to access.  For example we may be debugging WASM code, running in a browser, or a remote computer.

Log files may be very large.  Consider examining only part of them with your tools, or searching them with grep.

Often, on wasm, only the main thread's logs are printed.  So if you are reading stdout, you are missing many logs that are being written by other threads.  So the output from other sources may be HIGHLY misleading.


Using this command ensures you get all the logwise logs from all threads, that are prior to `exfiltrate::begin`.  (Logs prior to this call are not captured; so users are instructed to make this call early in their program).

For more information on using logwise, try building the latest documentation for it.  Alternatively, some resources are
        * https://sealedabstract.com/code/logwise
        * https://docs.rs/logwise/latest/logwise/
"
    }

    fn execute(&self, _args: Vec<String>) -> Result<Response, Response> {
        let logger = &LOGGER;
        let clone_all_logs = logger.records.with_sync(|logs| logs.clone());
        let mut str = String::new();
        for log in clone_all_logs {
            str.push_str(&log.to_string());
            str.push('\n');
        }
        let response = Response::Files(vec![FileInfo::new(
            "log".to_string(),
            None,
            str.as_bytes().to_vec(),
        )]);
        Ok(response)
    }
}
