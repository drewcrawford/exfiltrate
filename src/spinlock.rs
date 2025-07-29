use std::cell::UnsafeCell;

#[derive(Debug)]
pub struct Spinlock<T> {
    lock: std::sync::atomic::AtomicBool,
    data: UnsafeCell<T>,
}

impl<T> Spinlock<T> {
    pub fn new(data: T) -> Self {
        Spinlock {
            lock: std::sync::atomic::AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    fn lock(&self) {
        while self.lock.compare_exchange_weak(false, true, std::sync::atomic::Ordering::Acquire, std::sync::atomic::Ordering::Relaxed).is_err() {
            std::hint::spin_loop();
        }
    }

    fn unlock(&self) {
        self.lock.store(false, std::sync::atomic::Ordering::Release);
    }

    pub fn with_mut<F: FnOnce(&mut T) -> R, R>(&self, f: F) -> R {
        self.lock();
        let result = f(unsafe { &mut *self.data.get() });
        self.unlock();
        result
    }
}

unsafe impl <T: Send> Send for Spinlock<T> {}
unsafe impl <T: Sync> Sync for Spinlock<T> {}