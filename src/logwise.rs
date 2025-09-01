//! Logwise logging integration for exfiltrate.
//!
//! This module provides integration with the logwise logging framework, allowing
//! log records to be captured and forwarded through the exfiltrate system via
//! JSON-RPC notifications. This is particularly useful for capturing and analyzing
//! logs from remote or embedded systems.
//!
//! # Overview
//!
//! The module implements a custom `Logger` that intercepts logwise log records
//! and forwards them as JSON-RPC notifications through the internal proxy system.
//! This allows logs to be collected, analyzed, and stored by external systems
//! that connect to the exfiltrate proxy.
//!
//! # Architecture
//!
//! The logging capture works by:
//! 1. Installing a custom `ForwardingLogger` as a global logger in logwise
//! 2. Intercepting all log records that flow through logwise
//! 3. Converting log records to JSON-RPC notifications
//! 4. Forwarding notifications through the internal proxy for external consumption
//!
//! # Examples
//!
//! ## Basic usage
//!
//! ```
//! # // This example won't actually run the capture since it requires a proxy connection
//! # fn main() {
//! // Note: begin_capture() requires the logwise feature to be enabled
//! # #[cfg(feature = "logwise")]
//! # {
//! // Start capturing logwise logs
//! // exfiltrate::logwise::begin_capture();
//! 
//! // Example of what would be captured (using logwise directly)
//! // logwise::info_sync!("This log would be captured", user="alice");
//! # }
//! # }
//! ```
//!
//! ## With complex types
//!
//! ```
//! # fn main() {
//! # #[cfg(feature = "logwise")] 
//! # {
//! #[derive(Debug)]
//! struct ComplexData { 
//!     value: i32 
//! }
//! 
//! let data = ComplexData { value: 42 };
//! 
//! // Complex types need to be wrapped with LogIt for privacy control
//! // This demonstrates the syntax, though actual capture requires begin_capture()
//! // logwise::info_sync!(
//! //     "Processing data: {data}",
//! //     data = logwise::privacy::LogIt(&data)
//! // );
//! # }
//! # }
//! ```
//!
//! # Privacy Considerations
//!
//! The logwise framework includes a dual logging system with privacy controls.
//! When using this module, be aware that:
//! - All captured logs are forwarded to external systems
//! - Sensitive data should use appropriate logwise privacy wrappers
//! - The forwarding respects logwise's privacy settings and redaction rules

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use logwise::{LogRecord, Logger};
use crate::internal_proxy::InternalProxy;
use crate::jrpc::Notification;

/// A logger implementation that forwards log records through the exfiltrate system.
///
/// This logger intercepts logwise log records and converts them to JSON-RPC
/// notifications that are sent through the internal proxy. This allows external
/// systems to receive and process log data in real-time.
///
/// # Implementation Details
///
/// The logger implements both synchronous and asynchronous logging methods,
/// though both currently use the same underlying synchronous implementation
/// for simplicity and consistency.
#[derive(Debug)]
struct ForwardingLogger {

}

impl Logger for ForwardingLogger {
    /// Processes a completed log record synchronously.
    ///
    /// Converts the log record to a string representation and sends it as a
    /// JSON-RPC notification with the method name "exfiltrate/logwise/record".
    fn finish_log_record(&self, record: LogRecord) {
        let record = record.to_string();
        //presumably logwise can handle the print for us
        // crate::logging::log(&format!("Logwise: {}", record));
        let n = Notification::new("exfiltrate/logwise/record".to_string(), Some(record.into()));
        InternalProxy::current().buffer_notification(n);
    }

    /// Processes a completed log record asynchronously.
    ///
    /// Currently delegates to the synchronous implementation wrapped in an async block.
    /// This ensures consistent behavior between sync and async logging paths.
    fn finish_log_record_async<'s>(&'s self, record: LogRecord) -> Pin<Box<dyn Future<Output=()> + Send + 's>> {
        Box::pin(async move{self.finish_log_record(record)})
    }

    /// Prepares the logger for shutdown.
    ///
    /// This method is called when the logging system is shutting down.
    /// Currently a no-op as the forwarding logger doesn't maintain any
    /// resources that need explicit cleanup.
    fn prepare_to_die(&self) {
        // No cleanup needed for the forwarding logger
    }
}

impl ForwardingLogger {
    /// Installs the forwarding logger as a global logger in logwise.
    ///
    /// This method:
    /// 1. Sends a notification that log capture is starting
    /// 2. Creates a new ForwardingLogger instance
    /// 3. Registers it with logwise as a global logger
    ///
    /// After installation, all logwise log records will be forwarded through
    /// the exfiltrate system.
    fn install() {
        let n = Notification::new("exfiltrate/logwise/new".to_string(),None);
        InternalProxy::current().buffer_notification(n);
        let f = ForwardingLogger{};
        logwise::add_global_logger(Arc::new(f));
    }

}

/// Begins capturing logwise log records for forwarding through exfiltrate.
///
/// This function installs a custom logger that intercepts all logwise log records
/// and forwards them as JSON-RPC notifications through the internal proxy system.
/// This allows external systems to receive and process log data.
///
/// # Effects
///
/// After calling this function:
/// - All logwise log records will be captured and forwarded
/// - A notification is sent indicating log capture has started
/// - A message is printed to stderr confirming capture has begun
///
/// # Thread Safety
///
/// This function can be called from any thread, but should typically only be
/// called once at application startup. Multiple calls are safe but will result
/// in duplicate log forwarding.
///
/// # Examples
///
/// ## Basic initialization
///
/// ```
/// # fn main() {
/// # #[cfg(feature = "logwise")]
/// # {
/// // Start capturing logs at application startup
/// // Note: This would actually start capture in a real application
/// // exfiltrate::logwise::begin_capture();
///
/// // All subsequent logwise logs would be forwarded
/// // logwise::info_sync!("Application started");
/// // logwise::debug_sync!("Debug mode enabled", verbose=true);
/// # }
/// # }
/// ```
///
/// ## With error handling
///
/// ```
/// # fn main() {
/// # #[cfg(feature = "logwise")]
/// # {
/// use std::fs::File;
/// use std::io::Error;
///
/// // In a real application, you would call:
/// // exfiltrate::logwise::begin_capture();
///
/// match File::open("nonexistent.json") {
///     Ok(_) => {
///         // logwise::info_sync!("Config loaded successfully")
///     },
///     Err(e) => {
///         // logwise::error_sync!("Failed to load config: {error}", error=e.to_string())
///     },
/// }
/// # }
/// # }
/// ```
///
/// ## Integration pattern
///
/// ```
/// # fn main() {
/// # #[cfg(feature = "logwise")]
/// # {
/// // Example of how to structure log capture initialization
/// fn initialize_logging() {
///     // This would be called early in your application:
///     // exfiltrate::logwise::begin_capture();
///     
///     // Then you can use logwise macros throughout your code:
///     // logwise::info_sync!("Logging system initialized");
/// }
///
/// // Call at application startup
/// initialize_logging();
/// # }
/// # }
/// ```
pub fn begin_capture() {
    ForwardingLogger::install();
    eprintln!("Logwise capture started");
}