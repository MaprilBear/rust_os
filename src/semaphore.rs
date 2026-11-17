//! Counting semaphores, split in two:
//!
//!   * `KSemaphore` - the kernel object (count + wait queue). Always a `static`
//!     placed in the `.ksem` section, which link.ld puts in kernel RAM, so
//!     unprivileged code cannot read or corrupt the wait queue.
//!   * `Semaphore` - the user handle: just the address of its `KSemaphore`.
//!     It is `const`-constructible, so it can be a field of any `static`.
//!
//! Create both at once with the `semaphore!(count)` macro. A `const fn` cannot
//! do this on its own: it cannot emit a second static into a different link
//! section. The macro can, and each expansion gets its own static.
//!
//!     static TX_DONE: Semaphore = semaphore!(0);
//!     pub static UARTE0: Uarte = Uarte::new(BASE, semaphore!(1), semaphore!(0));
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
use core::arch::asm;
use core::cell::UnsafeCell;
use core::mem::size_of;
use core::ptr::addr_of;

use crate::scheduler::{scheduler, Scheduler, CURR_TCB};
use crate::syscall::{sys_sem_give, sys_sem_take};
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
        $crate::semaphore::Semaphore::__from_kernel_object(&KOBJ)
    }};
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Kernel side. Only call these from privileged code (the SVC dispatcher, ISRs).
///////////////////////////////////////////////////////////////////////////////////////////////////

/// A handle is only valid if it is exactly the address of a `KSemaphore` in `.ksem`.
/// User code controls the value, so this check is the whole security boundary.
fn lookup(handle: u32) -> Option<&'static KSemaphore> {
    let (start, end) = (addr_of!(_sksem) as u32, addr_of!(_eksem) as u32);
    let size = size_of::<KSemaphore>() as u32;
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

pub fn ksem_take(handle: u32) -> TakeResult {
    critical_section::with(|_| {
        let Some(sem) = lookup(handle) else {
            return TakeResult::Error;
        };
        let st = unsafe { &mut *sem.state.get() };
        if st.count <= 0 && unsafe { CURR_TCB.is_null() } {
            return TakeResult::Error; // would block, but there is no thread to block
        }
        st.count -= 1;
        if st.count < 0 {
            unsafe {
                scheduler().block();
                st.waiters.push_back(scheduler().current_thread());
            }
            TakeResult::Blocked
        } else {
            TakeResult::Acquired
        }
    })
}

/// `None` for a bad handle; `Some(true)` if a waiter was woken (the caller must
/// then pend PendSV so a higher-priority waiter can preempt).
pub fn ksem_give(handle: u32) -> Option<bool> {
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
// User handle
///////////////////////////////////////////////////////////////////////////////////////////////////

/// IPSR is readable from unprivileged code. Nonzero means we are in an exception handler.
#[inline(always)]
fn in_handler_mode() -> bool {
    let ipsr: u32;
    unsafe {
        asm!("mrs {}, ipsr", out(reg) ipsr, options(nomem, nostack, preserves_flags));
    }
    ipsr & 0x1FF != 0
}

pub struct Semaphore {
    // A raw pointer, never dereferenced here: it points into kernel RAM, which
    // unprivileged code cannot read. (A `&KSemaphore` would let the compiler
    // insert speculative loads.) It is only passed to the kernel as a handle.
    kobj: *const KSemaphore,
}

// The handle is just an address; all shared state is in the kernel and guarded there.
unsafe impl Sync for Semaphore {}
unsafe impl Send for Semaphore {}

#[allow(dead_code)]
impl Semaphore {
    #[doc(hidden)]
    pub const fn __from_kernel_object(kobj: &'static KSemaphore) -> Self {
        Self {
            kobj: kobj as *const KSemaphore,
        }
    }

    fn handle(&self) -> u32 {
        self.kobj as usize as u32
    }

    /// Blocks the calling thread if the count is exhausted. Thread context only.
    pub fn take(&self) {
        debug_assert!(!in_handler_mode(), "Semaphore::take from an ISR");
        assert!(sys_sem_take(self.handle()) == 0, "bad semaphore handle");
    }

    /// Callable from threads and from ISRs. A handler cannot issue `svc` at the
    /// same priority as SVC (that escalates to HardFault), so ISRs call the
    /// kernel directly.
    pub fn give(&self) {
        if in_handler_mode() {
            match ksem_give(self.handle()) {
                Some(true) => os_yield(),
                Some(false) => {}
                None => panic!("bad semaphore handle"),
            }
        } else {
            assert!(sys_sem_give(self.handle()) == 0, "bad semaphore handle");
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
