//! Counting semaphores, split in two:
//!
//!   * `KSemaphore` - the kernel object (count + wait queue). Always a `static`
//!     placed in the `.ksem` section, which link.ld puts in kernel RAM, so
//!     unprivileged code cannot read or corrupt the wait queue.
//!   * `userlib::Semaphore` - the user handle: just the address of its
//!     `KSemaphore`. It is `const`-constructible, so it can be a field of any
//!     `static`. Threads use it through syscalls; ISRs use `give_from_isr`.
//!
//! Create both at once with the `semaphore!(count)` macro. A `const fn` cannot
//! do this on its own: it cannot emit a second static into a different link
//! section. The macro can, and each expansion gets its own static.
//!
//!     static TX_DONE: userlib::Semaphore = kernel::semaphore!(0);
//!     pub static UARTE0: Uarte = Uarte::new(BASE, semaphore!(1), semaphore!(0));
//!
//! The macro expands in the *calling* crate, so `KOBJ` is not part of the kernel
//! crate's object code. link.ld still puts it in kernel RAM because it matches
//! `.ksem` by section name.
//!
//! Do not call `semaphore!` inside a `fn` that is run more than once (including
//! a `const fn` constructor used for several instances): every call would share
//! one kernel object. Call it where the static is defined.
//!
//! Semantics are the classic hand-off kind: `take` decrements and blocks if the
//! count went negative; `give` increments and, if someone is waiting, wakes the
//! oldest waiter, who then owns that unit. A woken thread resumes right after its
//! `svc` with nothing to retry.

use alloc::collections::VecDeque;
use core::cell::UnsafeCell;
use core::mem::size_of;
use core::ptr::addr_of;

use crate::scheduler::{scheduler, Scheduler};
use crate::{os_yield, ThreadId};

unsafe extern "C" {
    // Bounds of the `.ksem` input sections inside `.data` (link.ld).
    static _sksem: u8;
    static _eksem: u8;
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Kernel object
///////////////////////////////////////////////////////////////////////////////////////////////////

struct KSemState {
    count: isize,
    waiters: VecDeque<ThreadId>,
}

#[doc(hidden)]
pub struct KSemaphore {
    state: UnsafeCell<KSemState>,
}

// Only touched from privileged code, inside critical sections.
unsafe impl Sync for KSemaphore {}

impl KSemaphore {
    #[doc(hidden)]
    pub const fn new(count: isize) -> Self {
        Self {
            state: UnsafeCell::new(KSemState {
                count,
                waiters: VecDeque::new(),
            }),
        }
    }
}

/// Declares the kernel object as a static in kernel RAM and evaluates to the
/// user handle for it. Usable in `static` and `const` initialisers.
#[macro_export]
macro_rules! semaphore {
    ($count:expr) => {{
        #[link_section = ".ksem"]
        static KOBJ: $crate::semaphore::KSemaphore = $crate::semaphore::KSemaphore::new($count);
        $crate::semaphore::__handle_for(&KOBJ)
    }};
}

#[doc(hidden)]
pub const fn __handle_for(k: &'static KSemaphore) -> userlib::Semaphore {
    userlib::Semaphore::from_kernel_object(k as *const KSemaphore as *const ())
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Kernel side. Only call these from privileged code (the SVC dispatcher, ISRs).
///////////////////////////////////////////////////////////////////////////////////////////////////

/// A handle is only valid if it is exactly the address of a `KSemaphore` in `.ksem`.
/// User code controls the value, so this check is the whole security boundary.
fn lookup(handle: usize) -> Option<&'static KSemaphore> {
    let (start, end) = (addr_of!(_sksem) as usize, addr_of!(_eksem) as usize);
    let size = size_of::<KSemaphore>();
    if handle >= start && handle < end && (handle - start).is_multiple_of(size) {
        Some(unsafe { &*(handle as *const KSemaphore) })
    } else {
        None
    }
}

pub enum TakeResult {
    Acquired,
    /// The calling thread is now `Blocked` and queued; the caller must pend PendSV.
    Blocked,
    /// Bad handle, or the caller would block with no running thread (boot code).
    Error,
}

pub fn ksem_take(handle: usize) -> TakeResult {
    critical_section::with(|_| {
        let Some(sem) = lookup(handle) else {
            return TakeResult::Error;
        };
        let st = unsafe { &mut *sem.state.get() };
        if st.count > 0 {
            st.count -= 1;
            return TakeResult::Acquired;
        }
        // Would block: only a running thread can.
        let Some(current) = (unsafe { scheduler().current_thread() }) else {
            return TakeResult::Error;
        };
        st.count -= 1;
        unsafe { scheduler().block() };
        st.waiters.push_back(current);
        TakeResult::Blocked
    })
}

/// `None` for a bad handle; `Some(true)` if a waiter was woken (the caller must
/// then pend PendSV so a higher-priority waiter can preempt).
pub fn ksem_give(handle: usize) -> Option<bool> {
    critical_section::with(|_| {
        let st = unsafe { &mut *lookup(handle)?.state.get() };
        st.count += 1;
        Some(match st.waiters.pop_front() {
            Some(waiter) => {
                unsafe { scheduler().unblock(waiter) };
                true
            }
            None => false,
        })
    })
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// ISR side
///////////////////////////////////////////////////////////////////////////////////////////////////

/// `Semaphore::give` for interrupt handlers. A handler cannot issue `svc` at the
/// same priority as SVC (that escalates to HardFault), so ISRs, which run
/// privileged, call the kernel directly.
pub fn give_from_isr(sem: &userlib::Semaphore) {
    match ksem_give(sem.handle()) {
        Some(true) => os_yield(),
        Some(false) => {}
        None => panic!("bad semaphore handle"),
    }
}
