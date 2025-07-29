use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use serde_json::Value;
use crate::tools::{Argument, InputSchema, Tool, ToolCallError, ToolCallResponse};

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

#[derive(Debug, serde::Serialize)]
struct LogResponse {
    logs: Vec<String>,
    start_pos: usize,
    end_pos: usize,
    all_logs: usize,
}
pub struct LogwiseRead;

impl Tool for LogwiseRead {
    fn name(&self) -> &str {
        "logwise_read"
    }

    fn description(&self) -> &str {
        "Reads logs from logwise.

        Often logs are printed to console.  However some environments with complex redirect setups
        may only print logs from certain threads or may not be flushed.  This tool allows
        more direct access to the logs.

        Limitations: in order for logs to be available from this tool, the target application must
        a) log with logwise, and b) call `exfiltrate::logwise::begin_capture()` to begin redirecting
        logs into this tool.  Logs made before this call will not be available.
        "
    }

    fn input_schema(&self) -> InputSchema {
        InputSchema::new(vec![
            Argument::new("start_pos".to_string(), "integer".to_string(), "The position to start reading logs from.  If omitted, tails the logs.".to_string(), false),
            Argument::new("length".to_string(), "integer".to_string(), "The number of logs to read.  If omitted, defaults to 10.  If the combination of start_pos and length go out of bounds, return as many logs are in bounds.".to_string(), false),
        ])
    }

    fn call(&self, params: HashMap<String, Value>) -> Result<ToolCallResponse, ToolCallError> {
        let log_proxy = LogProxy::current().logs.lock().unwrap();
        let length = params.get("length")
            .and_then(|v| v.as_i64())
            .unwrap_or(10) as usize;

        let default_start_pos = log_proxy.len().saturating_sub(length);

        let start_pos = params.get("start_pos")
            .and_then(|v| v.as_i64())
            .map(|v| v as usize)
            .unwrap_or(default_start_pos);

        //adjust to make in bounds
        let start_pos = start_pos.min(log_proxy.len()).max(0);
        let end_pos = (start_pos + length).min(log_proxy.len());
        let logs = log_proxy[start_pos..end_pos].to_vec();
        let response = LogResponse {
            logs,
            start_pos,
            end_pos,
            all_logs: log_proxy.len(),
        };
        let response_text = serde_json::to_string(&response).unwrap();
        Ok(ToolCallResponse::new(vec![response_text.into()]))
    }

}
#[derive(Debug,serde::Serialize)]
struct MatchedLog {
    log: String,
    position: usize,
}

#[derive(Debug,serde::Serialize)]
struct LogwiseGrepResponse {
    all_logs: usize,
    matched_logs: Vec<MatchedLog>,
}

pub struct LogwiseGrep;
impl Tool for LogwiseGrep {
    fn name(&self) -> &str {
        "logwise_grep"
    }

    fn description(&self) -> &str {
        "Greps logs from logwise.

        Limitations: same as logwise_read.
        "
    }

    fn input_schema(&self) -> InputSchema {
        let pattern_doc = r#"A regular expression to search for in logs.  If no logs match, returns an empty list.

        logwise_grep uses the `regex` crate for regular expressions, which supports a wide range of features.  For more information, see https://docs.rs/regex/latest/regex/

        Typical logwise logs are in the following format:
        ```
         0 INFO: examples/log_exfiltration.rs:4:5 [0ns] MESSAGE
        ```

        Where:
        - `0` is the task ID.  You can use this to filter logs by task ID.
        - `INFO` is the log level
        - `examples/log_exfiltration.rs:4:5` is the file and line
        - `[0ns]` is the timestamp
        - `MESSAGE` is the log message
        "#;

        InputSchema::new(vec![
            Argument::new("pattern".to_string(), "string".to_string(), pattern_doc.to_string(), true),
        ])
    }

    fn call(&self, params: HashMap<String, Value>) -> Result<ToolCallResponse, ToolCallError> {
        let pattern = params.get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolCallError::new(vec!["No pattern".into()]))?;

        let regex = regex::Regex::new(pattern).map_err(|_| ToolCallError::new(vec!["Invalid regex".into()]))?;
        let log_proxy = LogProxy::current().logs.lock().unwrap();


        let logs: Vec<MatchedLog> = log_proxy.iter()
            .enumerate()
            .filter_map(|(i, log)| {
                if regex.is_match(log) {
                    Some(MatchedLog {
                        log: log.clone(),
                        position: i,
                    })
                } else {
                    None
                }
            })
            .collect();

        let response = LogwiseGrepResponse {
            all_logs: log_proxy.len(),
            matched_logs: logs,
        };
        let res = serde_json::to_string(&response).map_err(|e| ToolCallError::new(vec![e.to_string().into()]))?;
        Ok(ToolCallResponse::new(vec![res.into()]))
    }
}
