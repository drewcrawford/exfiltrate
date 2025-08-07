use std::pin::Pin;
use std::sync::Arc;
use logwise::{LogRecord, Logger};
use crate::internal_proxy::InternalProxy;
use crate::jrpc::Notification;

#[derive(Debug)]
struct ForwardingLogger {

}

impl Logger for ForwardingLogger {
    fn finish_log_record(&self, record: LogRecord) {
        let record = record.to_string();
        eprintln!("Logwise record: {}", record);
        let n = Notification::new("exfiltrate/logwise/record".to_string(), Some(record.into()));
        InternalProxy::current().buffer_notification(n);
    }

    fn finish_log_record_async<'s>(&'s self, record: LogRecord) -> Pin<Box<dyn Future<Output=()> + Send + 's>> {
        Box::pin(async move{self.finish_log_record(record)})
    }

    fn prepare_to_die(&self) {
        // ?
    }
}

impl ForwardingLogger {
    fn install() {
        let n = Notification::new("exfiltrate/logwise/new".to_string(),None);
        InternalProxy::current().buffer_notification(n);
        let f = ForwardingLogger{};
        logwise::add_global_logger(Arc::new(f));
    }

}

pub fn begin_capture() {
    ForwardingLogger::install();
    eprintln!("Logwise capture started");
}