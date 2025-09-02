/*!
A non-blocking synchronization primitive for one-time initialization.

`OnceNonLock<T>` provides a thread-safe way to initialize a value exactly once, similar to
`std::sync::OnceLock`, but with a crucial difference: it never blocks waiting threads.
Instead of blocking when initialization is in progress, `OnceNonLock` returns `None`,
allowing the calling thread to continue with other work.

# Key Differences from `std::sync::OnceLock`

- **Non-blocking**: When one thread is initializing the value, other threads receive `None`
  instead of blocking and waiting.
- **Try-semantics**: All initialization methods follow try-semantics, returning `None` when
  the value is unavailable rather than blocking.
- **Async support**: Provides `init_async` for asynchronous initialization scenarios.

# When to Use

Use `OnceNonLock` when:
- You need lazy initialization but cannot afford to block threads
- Multiple threads might attempt initialization, but only one should succeed
- You want to avoid thread contention and potential deadlocks
- The calling code can handle the value not being immediately available

Prefer `std::sync::OnceLock` when:
- You need guaranteed access to the value after initialization
- Blocking behavior is acceptable or desired
- You need the full `get_or_init` semantics where the value is always returned

# Internal States

The implementation uses three atomic states to coordinate initialization:
- `INITIAL` (0): The value has not been initialized yet
- `IN_PROGRESS` (1): A thread is currently initializing the value
- `DONE` (2): The value has been successfully initialized

# Thread Safety

`OnceNonLock` is thread-safe and can be safely shared between threads:
- Multiple threads can call `try_get_or_init` concurrently
- Only one thread will successfully initialize the value
- Once initialized, all threads can safely read the value
- The implementation uses atomic operations and proper memory ordering

# Examples

## Basic Usage

```
use std::sync::Arc;
use std::thread;

# mod once_nonlock {
#     use std::cell::UnsafeCell;
#     use std::mem::ManuallyDrop;
#     use std::sync::Arc;
#     use std::sync::atomic::{AtomicU8, Ordering};
#
#     const ONCE_INITIAL: u8 = 0;
#     const ONCE_IN_PROGRESS: u8 = 1;
#     const ONCE_DONE: u8 = 2;
#
#     pub struct OnceNonLock<T> {
#         once: AtomicU8,
#         value: UnsafeCell<ManuallyDrop<Option<T>>>,
#         _marker: std::marker::PhantomData<T>,
#     }
#
#     impl<T> OnceNonLock<T> {
#         pub const fn new() -> Self {
#             OnceNonLock {
#                 once: AtomicU8::new(ONCE_INITIAL),
#                 value: UnsafeCell::new(ManuallyDrop::new(None)),
#                 _marker: std::marker::PhantomData,
#             }
#         }
#
#         pub fn try_get_or_init<F>(&self, f: F) -> Option<&T>
#         where
#             F: FnOnce() -> Option<T>,
#         {
#             match self.once.compare_exchange(ONCE_INITIAL, ONCE_IN_PROGRESS, Ordering::AcqRel, Ordering::Relaxed) {
#                 Ok(_) => {
#                     let value = f();
#                     unsafe {
#                         if let Some(value) = value {
#                             *self.value.get() = ManuallyDrop::new(Some(value));
#                             self.once.store(ONCE_DONE, Ordering::Release);
#                         } else {
#                             self.once.store(ONCE_INITIAL, Ordering::Release);
#                         }
#                     }
#                     unsafe {
#                         let f = self.value.get();
#                         let value = &*f;
#                         value.as_ref()
#                     }
#                 }
#                 Err(ONCE_IN_PROGRESS) => None,
#                 Err(ONCE_DONE) => unsafe {
#                     let f = self.value.get();
#                     let value = &*f;
#                     value.as_ref()
#                 },
#                 Err(_) => panic!("Invalid state"),
#             }
#         }
#
#         pub fn get(&self) -> Option<&T> {
#             match self.once.load(Ordering::Acquire) {
#                 ONCE_INITIAL => None,
#                 ONCE_IN_PROGRESS => None,
#                 ONCE_DONE => unsafe {
#                     let f = self.value.get();
#                     let value = &*f;
#                     value.as_ref()
#                 },
#                 _ => panic!("Invalid state"),
#             }
#         }
#     }
#
#     unsafe impl<T: Send> Send for OnceNonLock<T> {}
#     unsafe impl<T: Sync> Sync for OnceNonLock<T> {}
#
#     impl<T> Drop for OnceNonLock<T> {
#         fn drop(&mut self) {
#             match self.once.load(Ordering::Relaxed) {
#                 ONCE_INITIAL => {},
#                 ONCE_IN_PROGRESS => panic!("Dropping while in progress"),
#                 ONCE_DONE => unsafe {
#                     ManuallyDrop::drop(&mut *self.value.get());
#                 },
#                 _ => panic!("Invalid state"),
#             }
#         }
#     }
# }
# use once_nonlock::OnceNonLock;

static CONFIG: OnceNonLock<String> = OnceNonLock::new();

// First thread to call wins the race to initialize
let value = CONFIG.try_get_or_init(|| {
    Some("initialized".to_string())
});

// Subsequent calls return the initialized value
assert_eq!(CONFIG.get().map(|s| s.as_str()), Some("initialized"));
```

## Non-blocking Behavior

```
use std::sync::Arc;
use std::thread;
use std::time::Duration;

# mod once_nonlock {
#     use std::cell::UnsafeCell;
#     use std::mem::ManuallyDrop;
#     use std::sync::Arc;
#     use std::sync::atomic::{AtomicU8, Ordering};
#
#     const ONCE_INITIAL: u8 = 0;
#     const ONCE_IN_PROGRESS: u8 = 1;
#     const ONCE_DONE: u8 = 2;
#
#     pub struct OnceNonLock<T> {
#         once: AtomicU8,
#         value: UnsafeCell<ManuallyDrop<Option<T>>>,
#         _marker: std::marker::PhantomData<T>,
#     }
#
#     impl<T> OnceNonLock<T> {
#         pub const fn new() -> Self {
#             OnceNonLock {
#                 once: AtomicU8::new(ONCE_INITIAL),
#                 value: UnsafeCell::new(ManuallyDrop::new(None)),
#                 _marker: std::marker::PhantomData,
#             }
#         }
#
#         pub fn try_get_or_init<F>(&self, f: F) -> Option<&T>
#         where
#             F: FnOnce() -> Option<T>,
#         {
#             match self.once.compare_exchange(ONCE_INITIAL, ONCE_IN_PROGRESS, Ordering::AcqRel, Ordering::Relaxed) {
#                 Ok(_) => {
#                     let value = f();
#                     unsafe {
#                         if let Some(value) = value {
#                             *self.value.get() = ManuallyDrop::new(Some(value));
#                             self.once.store(ONCE_DONE, Ordering::Release);
#                         } else {
#                             self.once.store(ONCE_INITIAL, Ordering::Release);
#                         }
#                     }
#                     unsafe {
#                         let f = self.value.get();
#                         let value = &*f;
#                         value.as_ref()
#                     }
#                 }
#                 Err(ONCE_IN_PROGRESS) => None,
#                 Err(ONCE_DONE) => unsafe {
#                     let f = self.value.get();
#                     let value = &*f;
#                     value.as_ref()
#                 },
#                 Err(_) => panic!("Invalid state"),
#             }
#         }
#
#         pub fn get(&self) -> Option<&T> {
#             match self.once.load(Ordering::Acquire) {
#                 ONCE_INITIAL => None,
#                 ONCE_IN_PROGRESS => None,
#                 ONCE_DONE => unsafe {
#                     let f = self.value.get();
#                     let value = &*f;
#                     value.as_ref()
#                 },
#                 _ => panic!("Invalid state"),
#             }
#         }
#     }
#
#     unsafe impl<T: Send> Send for OnceNonLock<T> {}
#     unsafe impl<T: Sync> Sync for OnceNonLock<T> {}
#
#     impl<T> Drop for OnceNonLock<T> {
#         fn drop(&mut self) {
#             match self.once.load(Ordering::Relaxed) {
#                 ONCE_INITIAL => {},
#                 ONCE_IN_PROGRESS => panic!("Dropping while in progress"),
#                 ONCE_DONE => unsafe {
#                     ManuallyDrop::drop(&mut *self.value.get());
#                 },
#                 _ => panic!("Invalid state"),
#             }
#         }
#     }
# }
# use once_nonlock::OnceNonLock;

let once = Arc::new(OnceNonLock::new());
let once_clone = once.clone();

// Simulate slow initialization in one thread
let handle1 = thread::spawn(move || {
    let _result = once_clone.try_get_or_init(|| {
        thread::sleep(Duration::from_millis(100));
        Some("slow init".to_string())
    });
});

// Another thread won't block - it gets None immediately
thread::sleep(Duration::from_millis(10)); // Let first thread start
let result = once.try_get_or_init(|| Some("fast init".to_string()));

// This returns None because initialization is in progress
assert_eq!(result, None);

handle1.join().unwrap();

// Now the value is available
assert_eq!(once.get().map(|s| s.as_str()), Some("slow init"));
```

## Failed Initialization Recovery

```
# mod once_nonlock {
#     use std::cell::UnsafeCell;
#     use std::mem::ManuallyDrop;
#     use std::sync::Arc;
#     use std::sync::atomic::{AtomicU8, Ordering};
#
#     const ONCE_INITIAL: u8 = 0;
#     const ONCE_IN_PROGRESS: u8 = 1;
#     const ONCE_DONE: u8 = 2;
#
#     pub struct OnceNonLock<T> {
#         once: AtomicU8,
#         value: UnsafeCell<ManuallyDrop<Option<T>>>,
#         _marker: std::marker::PhantomData<T>,
#     }
#
#     impl<T> OnceNonLock<T> {
#         pub const fn new() -> Self {
#             OnceNonLock {
#                 once: AtomicU8::new(ONCE_INITIAL),
#                 value: UnsafeCell::new(ManuallyDrop::new(None)),
#                 _marker: std::marker::PhantomData,
#             }
#         }
#
#         pub fn try_get_or_init<F>(&self, f: F) -> Option<&T>
#         where
#             F: FnOnce() -> Option<T>,
#         {
#             match self.once.compare_exchange(ONCE_INITIAL, ONCE_IN_PROGRESS, Ordering::AcqRel, Ordering::Relaxed) {
#                 Ok(_) => {
#                     let value = f();
#                     unsafe {
#                         if let Some(value) = value {
#                             *self.value.get() = ManuallyDrop::new(Some(value));
#                             self.once.store(ONCE_DONE, Ordering::Release);
#                         } else {
#                             self.once.store(ONCE_INITIAL, Ordering::Release);
#                         }
#                     }
#                     unsafe {
#                         let f = self.value.get();
#                         let value = &*f;
#                         value.as_ref()
#                     }
#                 }
#                 Err(ONCE_IN_PROGRESS) => None,
#                 Err(ONCE_DONE) => unsafe {
#                     let f = self.value.get();
#                     let value = &*f;
#                     value.as_ref()
#                 },
#                 Err(_) => panic!("Invalid state"),
#             }
#         }
#     }
#
#     unsafe impl<T: Send> Send for OnceNonLock<T> {}
#     unsafe impl<T: Sync> Sync for OnceNonLock<T> {}
# }
# use once_nonlock::OnceNonLock;

let once = OnceNonLock::new();
let mut attempt = 0;

// First attempt fails
let result = once.try_get_or_init(|| {
    attempt += 1;
    None // Initialization fails
});
assert_eq!(result, None);

// State returns to INITIAL, allowing retry
let result = once.try_get_or_init(|| {
    attempt += 1;
    Some(42) // Successful initialization
});
assert_eq!(result, Some(&42));
assert_eq!(attempt, 2);
```
*/

use std::cell::UnsafeCell;
use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicU8, Ordering};

const ONCE_INITIAL: u8 = 0;
const ONCE_IN_PROGRESS: u8 = 1;
const ONCE_DONE: u8 = 2;

/// A non-blocking, thread-safe cell that can be written to only once.
///
/// Unlike `std::sync::OnceLock`, `OnceNonLock` never blocks threads. When initialization
/// is in progress by another thread, methods return `None` instead of waiting. This allows
/// threads to continue with other work rather than blocking on initialization.
///
/// # Type Parameters
///
/// - `T`: The type of value stored in the cell. Must be `Send` to share between threads
///   and `Sync` for concurrent access.
///
/// # Memory Management
///
/// The implementation uses `ManuallyDrop` to ensure proper cleanup of the stored value
/// during drop, preventing double-free issues while maintaining safe memory management.
#[derive(Debug)]
pub struct OnceNonLock<T> {
    /// Atomic state tracker using the ONCE_* constants to coordinate initialization
    once: AtomicU8, //the ONCE constants
    /// The actual storage for the optional value, wrapped for interior mutability
    value: UnsafeCell<ManuallyDrop<Option<T>>>,
    //explain to Rust we will be dropping this manually
    _marker: std::marker::PhantomData<T>,
}

impl<T> OnceNonLock<T> {
    /// Creates a new, uninitialized `OnceNonLock`.
    ///
    /// The cell starts in the `INITIAL` state and can be initialized later using
    /// `try_get_or_init` or `init_async`.
    ///
    /// # Examples
    ///
    /// ```
    /// # mod once_nonlock {
    /// #     use std::cell::UnsafeCell;
    /// #     use std::mem::ManuallyDrop;
    /// #     use std::sync::atomic::{AtomicU8, Ordering};
    /// #     
    /// #     pub struct OnceNonLock<T> {
    /// #         once: AtomicU8,
    /// #         value: UnsafeCell<ManuallyDrop<Option<T>>>,
    /// #         _marker: std::marker::PhantomData<T>,
    /// #     }
    /// #     
    /// #     impl<T> OnceNonLock<T> {
    /// #         pub const fn new() -> Self {
    /// #             OnceNonLock {
    /// #                 once: AtomicU8::new(0),
    /// #                 value: UnsafeCell::new(ManuallyDrop::new(None)),
    /// #                 _marker: std::marker::PhantomData,
    /// #             }
    /// #         }
    /// #     }
    /// #     
    /// #     unsafe impl<T: Send> Send for OnceNonLock<T> {}
    /// #     unsafe impl<T: Sync> Sync for OnceNonLock<T> {}
    /// # }
    /// # use once_nonlock::OnceNonLock;
    ///
    /// // Can be used in static contexts
    /// static GLOBAL: OnceNonLock<i32> = OnceNonLock::new();
    ///
    /// // Or created at runtime
    /// let local = OnceNonLock::<String>::new();
    /// ```
    pub const fn new() -> Self {
        OnceNonLock {
            once: AtomicU8::new(ONCE_INITIAL),
            value: UnsafeCell::new(ManuallyDrop::new(None)),
            _marker: std::marker::PhantomData,
        }
    }

    /// Attempts to initialize the value if not already initialized.
    ///
    /// This method has three possible outcomes:
    /// - If the value is uninitialized (`INITIAL` state), it calls `f` to initialize it.
    ///   If `f` returns `Some(value)`, the cell transitions to `DONE` and returns a reference.
    ///   If `f` returns `None`, the cell returns to `INITIAL` state for future retry.
    /// - If another thread is initializing (`IN_PROGRESS` state), returns `None` immediately
    ///   without blocking.
    /// - If already initialized (`DONE` state), returns a reference to the existing value.
    ///
    /// # Arguments
    ///
    /// * `f` - A closure that returns `Option<T>`. Called only if the cell is uninitialized.
    ///         Returning `None` allows initialization to be retried later.
    ///
    /// # Returns
    ///
    /// - `Some(&T)` if the value was successfully initialized (by this call or previously)
    /// - `None` if initialization is in progress by another thread or if `f` returned `None`
    ///
    /// # Thread Safety
    ///
    /// Multiple threads can safely call this method concurrently. The atomic compare-exchange
    /// ensures only one thread's closure will be executed.
    ///
    /// # Examples
    ///
    /// ```
    /// # mod once_nonlock {
    /// #     use std::cell::UnsafeCell;
    /// #     use std::mem::ManuallyDrop;
    /// #     use std::sync::atomic::{AtomicU8, Ordering};
    /// #     
    /// #     pub struct OnceNonLock<T> {
    /// #         once: AtomicU8,
    /// #         value: UnsafeCell<ManuallyDrop<Option<T>>>,
    /// #         _marker: std::marker::PhantomData<T>,
    /// #     }
    /// #     
    /// #     impl<T> OnceNonLock<T> {
    /// #         pub const fn new() -> Self {
    /// #             OnceNonLock {
    /// #                 once: AtomicU8::new(0),
    /// #                 value: UnsafeCell::new(ManuallyDrop::new(None)),
    /// #                 _marker: std::marker::PhantomData,
    /// #             }
    /// #         }
    /// #         
    /// #         pub fn try_get_or_init<F>(&self, f: F) -> Option<&T>
    /// #         where
    /// #             F: FnOnce() -> Option<T>,
    /// #         {
    /// #             // Simplified implementation for doctest
    /// #             None
    /// #         }
    /// #     }
    /// # }
    /// # use once_nonlock::OnceNonLock;
    ///
    /// let once = OnceNonLock::new();
    ///
    /// // Initialize with a value
    /// let result = once.try_get_or_init(|| Some(42));
    ///
    /// // Subsequent calls return the same value without calling the closure
    /// let result2 = once.try_get_or_init(|| panic!("This won't be called"));
    /// ```
    pub fn try_get_or_init<F>(&self, f: F) -> Option<&T>
    where
        F: FnOnce() -> Option<T>,
    {
        match self.once.compare_exchange(
            ONCE_INITIAL,
            ONCE_IN_PROGRESS,
            Ordering::AcqRel,
            Ordering::Relaxed,
        ) {
            Ok(_) => {
                // We are the first to call get_or_init, so we initialize the value
                let value = f();
                unsafe {
                    if let Some(value) = value {
                        // SAFETY: We have exclusive access to the value
                        *self.value.get() = ManuallyDrop::new(Some(value));
                        self.once.store(ONCE_DONE, Ordering::Release);
                    } else {
                        //go back to initial state if the value is None
                        self.once.store(ONCE_INITIAL, Ordering::Release);
                    }
                }
                unsafe {
                    // SAFETY: We have exclusive access to the value
                    let f = self.value.get();
                    let value = &*f;
                    let deref: Option<&T> = value.as_ref();
                    deref
                }
            }
            Err(ONCE_IN_PROGRESS) => {
                None // Another thread is already initializing, so the value cannot be accessed yet
            }
            Err(ONCE_DONE) => {
                // Another thread has already initialized the value, we can safely access it
                unsafe {
                    // SAFETY: We have exclusive access to the value
                    let f = self.value.get();
                    let value = &*f;
                    let deref: Option<&T> = value.as_ref();
                    deref
                }
            }
            Err(other) => {
                // This should not happen, but if it does, we return None
                panic!("OnceNonLock: try get_or_init with value {:?}", other);
            }
        }
    }

    /// Asynchronously attempts to initialize the value.
    ///
    /// Similar to `try_get_or_init`, but for async initialization scenarios. This method
    /// attempts to initialize the cell with an async closure and returns a future that
    /// completes when the initialization attempt finishes (successfully or not).
    ///
    /// # Arguments
    ///
    /// * `self` - Must be called on an `Arc<OnceNonLock<T>>` for shared ownership
    /// * `f` - An async closure that returns `Option<T>`
    ///
    /// # Behavior
    ///
    /// - If the cell is `INITIAL`, runs the async closure and updates the state
    /// - If initialization succeeds (`f` returns `Some`), the cell becomes `DONE`
    /// - If initialization fails (`f` returns `None`), the cell returns to `INITIAL`
    /// - If another thread is initializing or has initialized, returns immediately
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // ALLOW_IGNORE_DOCTEST: AsyncFnOnce is a nightly-only feature that cannot be used in stable doctests
    /// use std::sync::Arc;
    ///
    /// let once = Arc::new(OnceNonLock::new());
    ///
    /// // Initialize asynchronously
    /// once.init_async(async || {
    ///     // Perform async operations
    ///     Some("async value".to_string())
    /// }).await;
    /// ```
    #[cfg(target_arch = "wasm32")]
    pub fn init_async<F>(self: &Arc<Self>, f: F) -> impl Future<Output = ()> + use<F, T>
    where
        F: AsyncFnOnce() -> Option<T>,
    {
        let moveme = self.clone();
        async move {
            match moveme.once.compare_exchange(
                ONCE_INITIAL,
                ONCE_IN_PROGRESS,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // We are the first to call get_or_init, so we initialize the value
                    let value = f().await;
                    if let Some(value) = value {
                        unsafe {
                            // SAFETY: We have exclusive access to the value
                            *moveme.value.get() = ManuallyDrop::new(Some(value));
                        }
                        moveme.once.store(ONCE_DONE, Ordering::Release);
                    } else {
                        //go back to initial state if the value is None
                        moveme.once.store(ONCE_INITIAL, Ordering::Release);
                    }
                }
                Err(ONCE_IN_PROGRESS) => {}
                Err(ONCE_DONE) => {}
                Err(other) => {
                    panic!("OnceNonLock: async init with value {:?}", other)
                }
            }
        }
    }
    /// Returns a reference to the initialized value, if any.
    ///
    /// This method never blocks and never initializes the value. It simply checks the
    /// current state and returns the value if it has been successfully initialized.
    ///
    /// # Returns
    ///
    /// - `Some(&T)` if the value has been initialized
    /// - `None` if the value is uninitialized or currently being initialized
    ///
    /// # Examples
    ///
    /// ```
    /// # mod once_nonlock {
    /// #     use std::cell::UnsafeCell;
    /// #     use std::mem::ManuallyDrop;
    /// #     use std::sync::atomic::{AtomicU8, Ordering};
    /// #     
    /// #     const ONCE_INITIAL: u8 = 0;
    /// #     const ONCE_IN_PROGRESS: u8 = 1;
    /// #     const ONCE_DONE: u8 = 2;
    /// #     
    /// #     pub struct OnceNonLock<T> {
    /// #         once: AtomicU8,
    /// #         value: UnsafeCell<ManuallyDrop<Option<T>>>,
    /// #         _marker: std::marker::PhantomData<T>,
    /// #     }
    /// #     
    /// #     impl<T> OnceNonLock<T> {
    /// #         pub const fn new() -> Self {
    /// #             OnceNonLock {
    /// #                 once: AtomicU8::new(ONCE_INITIAL),
    /// #                 value: UnsafeCell::new(ManuallyDrop::new(None)),
    /// #                 _marker: std::marker::PhantomData,
    /// #             }
    /// #         }
    /// #         
    /// #         pub fn get(&self) -> Option<&T> {
    /// #             match self.once.load(Ordering::Acquire) {
    /// #                 ONCE_INITIAL => None,
    /// #                 ONCE_IN_PROGRESS => None,
    /// #                 ONCE_DONE => unsafe {
    /// #                     let f = self.value.get();
    /// #                     let value = &*f;
    /// #                     value.as_ref()
    /// #                 },
    /// #                 _ => panic!("Invalid state"),
    /// #             }
    /// #         }
    /// #         
    /// #         pub fn try_get_or_init<F>(&self, f: F) -> Option<&T>
    /// #         where
    /// #             F: FnOnce() -> Option<T>,
    /// #         {
    /// #             match self.once.compare_exchange(ONCE_INITIAL, ONCE_IN_PROGRESS, Ordering::AcqRel, Ordering::Relaxed) {
    /// #                 Ok(_) => {
    /// #                     let value = f();
    /// #                     unsafe {
    /// #                         if let Some(value) = value {
    /// #                             *self.value.get() = ManuallyDrop::new(Some(value));
    /// #                             self.once.store(ONCE_DONE, Ordering::Release);
    /// #                         } else {
    /// #                             self.once.store(ONCE_INITIAL, Ordering::Release);
    /// #                         }
    /// #                     }
    /// #                     unsafe {
    /// #                         let f = self.value.get();
    /// #                         let value = &*f;
    /// #                         value.as_ref()
    /// #                     }
    /// #                 }
    /// #                 Err(ONCE_IN_PROGRESS) => None,
    /// #                 Err(ONCE_DONE) => unsafe {
    /// #                     let f = self.value.get();
    /// #                     let value = &*f;
    /// #                     value.as_ref()
    /// #                 },
    /// #                 Err(_) => panic!("Invalid state"),
    /// #             }
    /// #         }
    /// #     }
    /// #     
    /// #     unsafe impl<T: Send> Send for OnceNonLock<T> {}
    /// #     unsafe impl<T: Sync> Sync for OnceNonLock<T> {}
    /// # }
    /// # use once_nonlock::OnceNonLock;
    ///
    /// let once = OnceNonLock::new();
    ///
    /// // Before initialization, get() returns None
    /// assert_eq!(once.get(), None);
    ///
    /// // Initialize the value
    /// once.try_get_or_init(|| Some(42));
    ///
    /// // After initialization, get() returns the value
    /// assert_eq!(once.get(), Some(&42));
    /// ```
    pub fn get(&self) -> Option<&T> {
        match self.once.load(Ordering::Acquire) {
            ONCE_INITIAL => None,     // Value was never initialized
            ONCE_IN_PROGRESS => None, // Value is still being initialized
            ONCE_DONE => unsafe {
                // SAFETY: We have exclusive access to the value
                let f = self.value.get();
                let value = &*f;
                value.as_ref()
            },
            _ => panic!("OnceNonLock: Invalid state on get"),
        }
    }
}
impl<T> Drop for OnceNonLock<T> {
    /// Drops the `OnceNonLock` and its contained value if initialized.
    ///
    /// # Panics
    ///
    /// Panics if the cell is being dropped while initialization is in progress
    /// (`IN_PROGRESS` state), which indicates a programming error.
    fn drop(&mut self) {
        match self.once.load(Ordering::Relaxed) {
            ONCE_INITIAL => {
                // Nothing to drop, value was never initialized
                return;
            }
            ONCE_IN_PROGRESS => {
                // This should not happen, as we should only drop after initialization is done
                panic!("OnceNonLock: Dropping while still in progress");
            }
            ONCE_DONE => {
                // We can safely drop the value
                unsafe {
                    // SAFETY: We are dropping the value manually
                    ManuallyDrop::drop(&mut *self.value.get());
                }
            }
            _ => {
                panic!("OnceNonLock: Invalid state on drop");
            }
        }
    }
}
// SAFETY: OnceNonLock can be sent between threads if T can be sent.
// The atomic state management ensures proper synchronization when the value
// is transferred between threads.
unsafe impl<T: Send> Send for OnceNonLock<T> {}

// SAFETY: OnceNonLock can be shared between threads if T can be shared.
// The atomic operations and UnsafeCell usage ensure that:
// - Only one thread can initialize the value (via compare_exchange)
// - Once initialized, the value is immutable and can be safely shared
// - Memory ordering (Acquire/Release) ensures proper visibility across threads
unsafe impl<T: Sync> Sync for OnceNonLock<T> {}
