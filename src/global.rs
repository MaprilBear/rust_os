use core::cell::UnsafeCell;

pub struct Global<T>(UnsafeCell<T>);

// Safety: single core; callers must ensure exclusive access (see below).
unsafe impl<T> Sync for Global<T> {}

impl<T> Global<T> {
    pub const fn new(v: T) -> Self {
        Self(UnsafeCell::new(v))
    }
    pub fn as_ptr(&self) -> *mut T {
        self.0.get()
    }
}
