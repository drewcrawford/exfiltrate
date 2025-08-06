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
        while self.lock.compare_exchange(false, true, std::sync::atomic::Ordering::Acquire, std::sync::atomic::Ordering::Relaxed).is_err() {
            super::logging::log(&format!("spinlock ATTEMPTING lock on type {}", std::any::type_name::<T>()));

            let mut logged = false;
            if !logged {
                logged = true;
                super::logging::log(&format!("spinlock SPINNING on type {}", std::any::type_name::<T>()));
            }
            std::hint::spin_loop();
        }
        super::logging::log(&format!("spinlock LOCK on type {}", std::any::type_name::<T>()));

    }

    fn unlock(&self) {
        super::logging::log(&format!("spinlock UNLOCK on type {}", std::any::type_name::<T>()));
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