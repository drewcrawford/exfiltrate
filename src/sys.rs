//! Platform abstraction layer for system APIs.
//!
//! This module provides unified access to platform-specific implementations of
//! common system APIs like time and threading. It automatically selects the
//! appropriate implementation based on the compilation target, allowing the rest
//! of the codebase to use these APIs without platform-specific conditionals.
//!
//! # Overview
//!
//! The module re-exports different implementations depending on the target:
//! - **Native platforms**: Uses the standard library implementations
//! - **WebAssembly**: Uses WebAssembly-compatible alternatives
//!
//! This abstraction is crucial for the exfiltrate library's ability to run
//! both as a native application and in WebAssembly environments (like browsers),
//! which is essential for its debugging and MCP server embedding capabilities.
//!
//! # Platform Implementations
//!
//! ## Time API
//! - **Native**: `std::time` - Full standard library time support
//! - **WASM**: `web_time` - Browser-compatible time API using performance.now()
//!
//! ## Threading API  
//! - **Native**: `std::thread` - OS threads with full standard library support
//! - **WASM**: `wasm_thread` - Web Workers-based threading for browsers
//!
//! # Design Philosophy
//!
//! As mentioned in the project README, this codebase deliberately avoids tokio
//! and async runtimes in favor of threads. This module ensures that threading
//! works consistently across all platforms, including WebAssembly where traditional
//! threads aren't available. "Threads for everyone" means using Web Workers on
//! WASM and OS threads on native platforms.
//!
//! # Examples
//!
//! ```
//! # mod sys {
//! #     pub use std::time;
//! #     pub use std::thread;
//! # }
//! use sys::time::{Duration, Instant};
//! use sys::thread;
//!
//! // Time operations work consistently across platforms
//! let start = Instant::now();
//! thread::sleep(Duration::from_millis(100));
//! let elapsed = start.elapsed();
//! println!("Operation took {:?}", elapsed);
//!
//! // Threading works on both native and WASM
//! let handle = thread::spawn(|| {
//!     println!("Running in a thread!");
//!     42
//! });
//! let result = handle.join().unwrap();
//! assert_eq!(result, 42);
//! ```
//!
//! # Usage Guidelines
//!
//! Always import time and thread APIs through this module rather than directly
//! from std or platform-specific crates. This ensures your code remains portable:

#[cfg(not(target_arch = "wasm32"))]
pub use std::time;
/// Platform-appropriate time API.
///
/// Re-exports the appropriate time implementation based on the target platform:
/// - Native platforms: `std::time`
/// - WebAssembly: `web_time` (browser-compatible)
///
/// # Available Types
///
/// The following types are available through this module:
/// - `Duration` - A span of time
/// - `Instant` - A measurement of monotonic time
/// - `SystemTime` - A measurement of system/wall-clock time (native only)
///
/// # Examples
///
/// ```
/// # mod sys {
/// #     pub use std::time;
/// # }
/// use sys::time::{Duration, Instant};
///
/// // Measure elapsed time
/// let start = Instant::now();
/// // ... do some work ...
/// let elapsed = start.elapsed();
///
/// // Create durations
/// let timeout = Duration::from_secs(5);
/// let delay = Duration::from_millis(100);
/// ```
///
/// # Platform Differences
///
/// While the API is consistent, there are some behavioral differences:
/// - **Resolution**: Browser time may have lower resolution due to security mitigations
/// - **SystemTime**: Not available in WASM environments
/// - **Monotonicity**: Guaranteed on both platforms but implementation differs
#[cfg(target_arch = "wasm32")]
pub use web_time as time;

/// Platform-appropriate threading API.
///
/// Re-exports the appropriate threading implementation based on the target platform:
/// - Native platforms: `std::thread` (OS threads)
/// - WebAssembly: `wasm_thread` (Web Workers)
///
/// # Available Functionality
///
/// The following threading primitives are available:
/// - `spawn` - Create a new thread
/// - `sleep` - Block the current thread for a duration
/// - `yield_now` - Yield execution to other threads
/// - `JoinHandle` - Handle for joining spawned threads
/// - `ThreadId` - Unique thread identifier
/// - `current` - Get current thread information
///
/// # Examples
///
/// ```
/// # mod sys {
/// #     pub use std::thread;
/// # }
/// use sys::thread;
/// use std::sync::Arc;
/// use std::sync::atomic::{AtomicUsize, Ordering};
///
/// // Spawn a thread
/// let handle = thread::spawn(|| {
///     println!("Hello from thread!");
///     42
/// });
///
/// // Wait for completion
/// let result = handle.join().unwrap();
/// assert_eq!(result, 42);
///
/// // Share data between threads
/// let counter = Arc::new(AtomicUsize::new(0));
/// let counter_clone = counter.clone();
///
/// thread::spawn(move || {
///     counter_clone.fetch_add(1, Ordering::SeqCst);
/// });
/// ```
///
/// # WASM Limitations
///
/// When running in WebAssembly:
/// - Threads are implemented using Web Workers
/// - `thread::park` and `thread::unpark` may have different semantics
/// - Thread-local storage has limitations
/// - Maximum thread count may be limited by browser
/// - Shared memory requires specific CORS headers
///
/// # Design Rationale
///
/// This abstraction supports the project's "threads for everyone" philosophy,
/// enabling consistent multi-threaded programming across all platforms without
/// requiring async/await or runtime dependencies like tokio.
#[cfg(target_arch = "wasm32")]
pub use wasm_thread as thread;

#[cfg(not(target_arch = "wasm32"))]
pub use std::thread;
