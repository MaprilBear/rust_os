use alloc::vec::Vec;
use core::cell::UnsafeCell;

use crate::{os_yield, ThreadId, ThreadState, CURR_TCB, SCHEDULER, TCB};

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
                let current = unsafe { &mut *CURR_TCB };
                let current_id = current.id;
                current.state = ThreadState::Blocked;
                raw.waiters.push(current_id);
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
                SCHEDULER.tcb_list.with(|tcb_list| {
                    let tcb: &mut TCB = tcb_list[waiter].as_mut().unwrap();
                    tcb.state = ThreadState::Ready;
                });
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
