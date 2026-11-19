//! System calls, kernel side.
//!
//! Threads run unprivileged and reach the kernel only through a syscall: one
//! argument in, one return value out (r0 on the target). The arch layer
//! decodes the trap (`arch/armv7m/svc.rs`) and calls [`dispatch`].
//!
//! The user-side wrappers (`sys_*`) and the `Syscall` numbers live in `userlib`,
//! so threads never execute code from this crate.

use core::ops::Range;
use core::sync::atomic::AtomicBool;

use userlib::Syscall;

use crate::arch;

static STARTED: AtomicBool = AtomicBool::new(false);

/// Who made a syscall.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(not(target_os = "none"), allow(dead_code))] // host: built by test support
pub enum Caller {
    /// The privileged boot code, before the scheduler runs.
    Boot,
    /// A thread.
    Thread,
}

/// True if `[ptr, ptr + len)` lies inside `region` (without overflowing).
fn in_range(ptr: usize, len: usize, region: Range<usize>) -> bool {
    let size = region.len();
    ptr >= region.start && len <= size && ptr - region.start <= size - len
}

/// The kernel must not trust pointers from user code: the MPU does not stop a
/// handler from dereferencing them.
#[allow(dead_code)]
fn user_can_read(ptr: usize, len: usize) -> bool {
    in_range(ptr, len, arch::flash_range()) || user_can_write(ptr, len)
}

#[allow(dead_code)]
fn user_can_write(ptr: usize, len: usize) -> bool {
    in_range(ptr, len, arch::uram_range())
}

mod syscall_impl {
    use core::sync::atomic::Ordering;

    use crate::{
        arch, os_yield,
        scheduler::{scheduler, Scheduler},
        semaphore::{ksem_give, ksem_take, TakeResult},
        syscall::{Caller, STARTED},
    };

    /// The calling thread cannot continue (e.g. it panicked). There is no thread
    /// teardown yet, so halt like the MemManage handler does.
    pub(super) fn fault() -> ! {
        match unsafe { scheduler().current_thread() } {
            Some(id) => dprintln!("thread {} faulted", id),
            None => dprintln!("fault reported with no running thread"),
        }
        arch::halt("thread faulted")
    }

    pub(super) fn start(caller: Caller) -> usize {
        // Only the boot code may do this, and only once.
        if caller != Caller::Boot || STARTED.swap(true, Ordering::SeqCst) {
            return 1;
        }
        arch::start_systick();
        os_yield();
        0
    }

    pub(super) fn r#yield() -> usize {
        os_yield();
        0
    }

    pub(super) fn sleep(ms: u32) -> usize {
        // Only a running thread can sleep (not boot code).
        if !unsafe { scheduler().sleep(ms) } {
            return 1;
        }
        os_yield();
        0
    }

    pub(super) fn sem_take(handle: usize) -> usize {
        match ksem_take(handle) {
            TakeResult::Acquired => 0,
            TakeResult::Error => 1,
            TakeResult::Blocked => {
                // The status is set now; the thread resumes after its `svc`
                // once `give` wakes it, with nothing left to retry.
                os_yield();
                0
            }
        }
    }

    pub(super) fn sem_give(handle: usize) -> usize {
        match ksem_give(handle) {
            Some(woke) => {
                if woke {
                    os_yield();
                }
                0
            }
            None => 1,
        }
    }
}

/// Run syscall number `num` with argument `arg0` for `caller` (the running
/// thread, or the boot code). Returns the caller's return value.
#[cfg_attr(not(target_os = "none"), allow(dead_code))] // host: called by test support
pub(crate) fn dispatch(num: u8, arg0: usize, caller: Caller) -> usize {
    let syscall_num = Syscall::try_from(num).unwrap_or(Syscall::Invalid);

    match syscall_num {
        Syscall::Start => syscall_impl::start(caller),
        Syscall::Yield => syscall_impl::r#yield(),
        Syscall::Sleep => syscall_impl::sleep(arg0 as u32),
        Syscall::SemTake => syscall_impl::sem_take(arg0),
        Syscall::SemGive => syscall_impl::sem_give(arg0),
        Syscall::Fault => syscall_impl::fault(),
        Syscall::Invalid => {
            dprintln!("Invalid SVC number {num}");
            0
        }
    }
}
