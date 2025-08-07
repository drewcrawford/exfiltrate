/*!
This is kinda like OnceLock, but it does not block
*/

use std::cell::UnsafeCell;
use std::mem::ManuallyDrop;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

const ONCE_INITIAL: u8 = 0;
const ONCE_IN_PROGRESS: u8 = 1;
const ONCE_DONE: u8 = 2;

#[derive(Debug)]
pub struct OnceNonLock<T> {
    once: AtomicU8, //the ONCE constants
    value: UnsafeCell<ManuallyDrop<Option<T>>>,
    //explain to Rust we will be dropping this manually
    _marker: std::marker::PhantomData<T>,
}

impl<T> OnceNonLock<T> {
    pub const fn new() -> Self {
        OnceNonLock {
            once: AtomicU8::new(ONCE_INITIAL),
            value: UnsafeCell::new(ManuallyDrop::new(None)),
            _marker: std::marker::PhantomData,
        }
    }

    pub fn try_get_or_init<F>(&self, f: F) -> Option<&T>
    where
        F: FnOnce() -> Option<T>,
    {
        match self.once.compare_exchange(ONCE_INITIAL, ONCE_IN_PROGRESS, Ordering::AcqRel, Ordering::Relaxed) {
            Ok(_) => {
                // We are the first to call get_or_init, so we initialize the value
                let value = f();
                unsafe {
                    if let Some(value) = value {
                        // SAFETY: We have exclusive access to the value
                        *self.value.get() = ManuallyDrop::new(Some(value));
                        self.once.store(ONCE_DONE, Ordering::Release);

                    }
                    else {
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

    pub fn init_async<F>(self: &Arc<Self>, f: F) -> impl Future<Output=()> + use<F,T> where
    F: AsyncFnOnce() -> Option<T> {
        let  moveme = self.clone();
        async move {
            match moveme.once.compare_exchange(ONCE_INITIAL, ONCE_IN_PROGRESS, Ordering::AcqRel, Ordering::Relaxed) {
                Ok(_) => {
                    // We are the first to call get_or_init, so we initialize the value
                    let value = f().await;
                    if let Some(value) = value {
                        unsafe {
                            // SAFETY: We have exclusive access to the value
                            *moveme.value.get() = ManuallyDrop::new(Some(value));
                        }
                        moveme.once.store(ONCE_DONE, Ordering::Release);
                    }
                    else {
                        //go back to initial state if the value is None
                        moveme.once.store(ONCE_INITIAL, Ordering::Release);
                    }
                }
                Err(ONCE_IN_PROGRESS) => {

                }
                Err(ONCE_DONE) => {

                }
                Err(other) => {
                    panic!("OnceNonLock: async init with value {:?}", other)
                }
            }
        }
    }
    pub fn get(&self) -> Option<&T> {
        match self.once.load(Ordering::Acquire) {
            ONCE_INITIAL => None, // Value was never initialized
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
impl <T> Drop for OnceNonLock<T> {
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
unsafe impl <T: Send> Send for OnceNonLock<T> {}
unsafe impl <T: Sync> Sync for OnceNonLock<T> {}