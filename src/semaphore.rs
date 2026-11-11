use alloc::vec::Vec;
use core::cell::UnsafeCell;

use crate::{
    os_yield,
    scheduler::{scheduler, Scheduler},
    ThreadId,
};

struct RawSemaphore {
    count: isize,
    waiters: Vec<ThreadId>,
}

pub struct Semaphore {
    raw: UnsafeCell<RawSemaphore>,
}

// To make the compiler happy, the semaphore implementation must be thread-safe through the use of
// critical sections.
unsafe impl Sync for Semaphore {}

#[allow(dead_code)]
impl Semaphore {
    pub const fn with_count(count: isize) -> Self {
        Self {
            raw: UnsafeCell::new(RawSemaphore {
                count,
                waiters: Vec::new(),
            }),
        }
    }

    pub fn take(&self) {
        let mut blocked = false;
        critical_section::with(|_| {
            let raw = unsafe { &mut *self.raw.get() };
            raw.count -= 1;
            if raw.count < 0 {
                unsafe { scheduler().block() };
                raw.waiters.push(unsafe { scheduler().current_thread() });
                blocked = true;
            }
        });
        if blocked {
            os_yield();
        }
    }

    pub fn give(&self) {
        let woke = critical_section::with(|_| {
            let raw = unsafe { &mut *self.raw.get() };
            raw.count += 1;
            if let Some(waiter) = raw.waiters.pop() {
                unsafe { scheduler().unblock(waiter) };
                true
            } else {
                false
            }
        });
        if woke {
            os_yield();
        }
    }

    pub fn with<F>(&self, f: F)
    where
        F: FnOnce(),
    {
        self.take();
        f();
        self.give();
    }
}
