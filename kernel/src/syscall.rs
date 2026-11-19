//! System calls, kernel side.
//!
//! Threads run unprivileged and reach the kernel only through a syscall: one
//! argument in, one return value out (r0 on the target). The arch layer
//! decodes the trap (`arch/armv7m/svc.rs`) and calls [`dispatch`].
//!
//! The user-side wrappers (`sys_*`) and the `Syscall` numbers live in `userlib`,
//! so threads never execute code from this crate.

use core::ptr::addr_of;
use core::sync::atomic::AtomicBool;

use userlib::Syscall;

static STARTED: AtomicBool = AtomicBool::new(false);

/// Who made a syscall.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Caller {
    /// The privileged boot code, before the scheduler runs.
    Boot,
    /// A thread.
    Thread,
}

unsafe extern "C" {
    // Defined in link.ld.
    static _flash_base: u8;
    static _flash_size: u8;
    static _uram_base: u8;
    static _uram_size: u8;
}

fn in_range(ptr: usize, len: usize, base: usize, size: usize) -> bool {
    ptr >= base && len <= size && ptr - base <= size - len
}

/// The kernel must not trust pointers from user code: the MPU does not stop a
/// handler from dereferencing them.
#[allow(dead_code)]
fn user_can_read(ptr: usize, len: usize) -> bool {
    let flash_base = addr_of!(_flash_base) as usize;
    let flash_size = addr_of!(_flash_size) as usize;
    in_range(ptr, len, flash_base, flash_size) || user_can_write(ptr, len)
}

#[allow(dead_code)]
fn user_can_write(ptr: usize, len: usize) -> bool {
    let uram_base = addr_of!(_uram_base) as usize;
    let uram_size = addr_of!(_uram_size) as usize;
    in_range(ptr, len, uram_base, uram_size)
}

mod syscall_impl {
    use core::sync::atomic::Ordering;

    use crate::{
        os_yield,
        scheduler::{scheduler, Scheduler},
        semaphore::{ksem_give, ksem_take, TakeResult},
        start_systick,
        syscall::{Caller, STARTED},
    };

    /// The calling thread cannot continue (e.g. it panicked). There is no thread
    /// teardown yet, so halt like the MemManage handler does.
    pub(super) fn fault() -> ! {
        match unsafe { scheduler().current_thread() } {
            Some(id) => dprintln!("thread {} faulted", id),
            None => dprintln!("fault reported with no running thread"),
        }
        #[allow(clippy::empty_loop)]
        loop {}
    }

    pub(super) fn start(caller: Caller) -> usize {
        // Only the boot code may do this, and only once.
        if caller != Caller::Boot || STARTED.swap(true, Ordering::SeqCst) {
            return 1;
        }
        start_systick();
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
