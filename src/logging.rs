//! Cross-platform logging utilities for the exfiltrate library.
//!
//! This module provides a simple, platform-agnostic logging interface that writes
//! to the appropriate output stream depending on the target platform. On standard
//! platforms, it writes to stderr, while on WebAssembly it uses the browser's
//! console API.
//!
//! # Overview
//!
//! The module is designed to provide basic logging functionality without the overhead
//! of a full logging framework. It's particularly useful for debugging and error
//! reporting in both native and WebAssembly environments.
//!
//! # Platform Behavior
//!
//! - **Native platforms**: Messages are written to stderr using `eprintln!`
//! - **WebAssembly**: Messages are written to the browser console using `console.log`
//!
//! # Examples
//!
//! ```
//! # mod logging {
//! #     pub fn log(str: &str) {
//! #         eprintln!("{}", str);
//! #     }
//! # }
//! # use logging::log;
//!
//! // Simple logging
//! log("Application started");
//!
//! // Logging with formatted strings
//! let port = 8080;
//! log(&format!("Server listening on port {}", port));
//!
//! // Error reporting
//! match std::fs::read_to_string("config.json") {
//!     Ok(_) => log("Configuration loaded"),
//!     Err(e) => log(&format!("Failed to load config: {}", e)),
//! }
//! ```

/// Logs a message to the platform-appropriate output stream.
///
/// This function provides cross-platform logging that automatically selects
/// the correct output mechanism based on the compilation target:
/// - On native platforms, writes to stderr
/// - On WebAssembly, writes to the browser console
///
/// # Arguments
///
/// * `str` - The message to log. This should be a string slice containing
///   the formatted message to output.
///
/// # Platform-specific behavior
///
/// ## Native platforms
/// Messages are written to stderr with a newline appended. This ensures
/// that log messages don't interfere with stdout, which may be used for
/// program output or inter-process communication.
///
/// ## WebAssembly
/// Messages are written to the browser's console using the Web API.
/// This makes debugging output visible in the browser's developer tools.
///
/// # Examples
///
/// ## Basic usage
/// ```
/// # mod logging {
/// #     pub fn log(str: &str) {
/// #         eprintln!("{}", str);
/// #     }
/// # }
/// # use logging::log;
///
/// log("Starting initialization");
/// log("Initialization complete");
/// ```
///
/// ## With runtime values
/// ```
/// # mod logging {
/// #     pub fn log(str: &str) {
/// #         eprintln!("{}", str);
/// #     }
/// # }
/// # use logging::log;
///
/// let user_count = 42;
/// log(&format!("Active users: {}", user_count));
///
/// let config = "production";
/// log(&format!("Running in {} mode", config));
/// ```
///
/// ## Error handling patterns
/// ```
/// # mod logging {
/// #     pub fn log(str: &str) {
/// #         eprintln!("{}", str);
/// #     }
/// # }
/// # use logging::log;
/// use std::fs::File;
///
/// match File::open("data.txt") {
///     Ok(_) => log("File opened successfully"),
///     Err(e) => log(&format!("Error opening file: {}", e)),
/// }
/// ```
///
/// # Performance considerations
///
/// This function performs I/O operations and should not be called in
/// performance-critical code paths. For high-frequency logging, consider
/// batching messages or using a more sophisticated logging framework
/// like logwise.
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
