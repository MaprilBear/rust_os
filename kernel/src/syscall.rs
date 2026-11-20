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

/// Test support: allow `Start` again.
#[cfg(all(not(target_os = "none"), any(test, feature = "test-support")))]
pub(crate) fn reset() {
    STARTED.store(false, core::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;
    use crate::ThreadState;
    // The dispatcher under test, not test_support's checked wrapper.
    use super::dispatch;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::string::String;

    fn call(num: Syscall, arg: usize, caller: Caller) -> usize {
        dispatch(num as u8, arg, caller)
    }

    // Start /////////////////////////////////////////////////////////////////

    #[test]
    fn start_from_boot_starts_the_tick_and_requests_a_switch() {
        let _k = lock();
        boot();
        assert_eq!(call(Syscall::Start, 0, Caller::Boot), 0);
        assert!(systick_started());
        assert!(pendsv_pending());
        assert_eq!(context_switch(), Some(0));
    }

    #[test]
    fn start_works_only_once() {
        let _k = lock();
        assert_eq!(call(Syscall::Start, 0, Caller::Boot), 0);
        assert_eq!(call(Syscall::Start, 0, Caller::Boot), 1);
    }

    #[test]
    fn start_from_a_thread_is_refused_and_does_not_use_up_the_boot_start() {
        let _k = lock();
        assert_eq!(call(Syscall::Start, 0, Caller::Thread), 1);
        assert!(!systick_started());
        assert!(!pendsv_pending());
        assert_eq!(call(Syscall::Start, 0, Caller::Boot), 0);
    }

    // Yield and Sleep ///////////////////////////////////////////////////////

    #[test]
    fn yield_requests_a_switch() {
        let _k = lock();
        boot();
        context_switch();
        assert_eq!(call(Syscall::Yield, 0, Caller::Thread), 0);
        assert!(pendsv_pending());
    }

    #[test]
    fn sleep_from_a_thread_succeeds() {
        let _k = lock();
        boot();
        let t = spawn(1);
        context_switch();
        assert_eq!(call(Syscall::Sleep, 4, Caller::Thread), 0);
        assert_eq!(thread_state(t), Some(ThreadState::Sleeping));
        assert!(pendsv_pending());
    }

    #[test]
    fn sleep_with_no_running_thread_fails_without_a_switch() {
        let _k = lock();
        boot();
        assert_eq!(call(Syscall::Sleep, 4, Caller::Boot), 1);
        assert!(!pendsv_pending());
        assert_eq!(thread_state(0), Some(ThreadState::Ready));
    }

    #[test]
    fn sleep_argument_is_truncated_to_32_bits() {
        let _k = lock();
        boot();
        let t = spawn(1);
        context_switch();
        // 2^32 + 1 ms is 1 ms: the target's registers are 32-bit.
        assert_eq!(call(Syscall::Sleep, (1usize << 32) + 1, Caller::Thread), 0);
        context_switch();
        tick();
        assert_eq!(thread_state(t), Some(ThreadState::Ready));
    }

    // Fault and invalid numbers /////////////////////////////////////////////

    fn fault_message(caller: Caller) -> (String, String) {
        let err = catch_unwind(AssertUnwindSafe(|| call(Syscall::Fault, 0, caller)))
            .expect_err("Fault must not return");
        let msg = err.downcast_ref::<String>().cloned().unwrap_or_default();
        (msg, take_log())
    }

    #[test]
    fn fault_from_a_thread_logs_its_id_and_halts() {
        let _k = lock();
        boot();
        let t = spawn(1);
        context_switch();
        let (panic, log) = fault_message(Caller::Thread);
        assert_eq!(panic, "kernel halted: thread faulted");
        assert_eq!(log, std::format!("thread {t} faulted\n"));
    }

    #[test]
    fn fault_with_no_running_thread_says_so_and_halts() {
        let _k = lock();
        let (panic, log) = fault_message(Caller::Boot);
        assert_eq!(panic, "kernel halted: thread faulted");
        assert_eq!(log, "fault reported with no running thread\n");
    }

    #[test]
    fn invalid_numbers_are_logged_and_change_nothing() {
        let _k = lock();
        boot();
        let t = spawn(1);
        context_switch();
        for num in [6u8, 7, 100, 254, 255] {
            dispatch(num, 0x1234, Caller::Thread);
            assert_eq!(take_log(), std::format!("Invalid SVC number {num}\n"));
        }
        // (What it returns is known issue 12.)
        assert_eq!(thread_state(t), Some(ThreadState::Running));
        assert!(!pendsv_pending());
        assert!(!systick_started());
    }

    /// Known issue 12: an invalid syscall returns 0, which callers read as
    /// success. It should return an error like the other failures (1).
    #[test]
    #[ignore = "known issue 12: invalid syscall numbers report success"]
    fn invalid_numbers_return_an_error() {
        let _k = lock();
        boot();
        context_switch();
        assert_eq!(dispatch(6, 0, Caller::Thread), 1);
        assert_eq!(dispatch(255, 0, Caller::Thread), 1);
    }

    // Pointer checks ////////////////////////////////////////////////////////

    #[test]
    fn in_range_accepts_buffers_wholly_inside_the_region() {
        let r = 0x100..0x200;
        assert!(in_range(0x100, 0x100, r.clone()), "the whole region");
        assert!(in_range(0x100, 1, r.clone()));
        assert!(in_range(0x1FF, 1, r.clone()), "last byte");
        assert!(in_range(0x180, 0x80, r.clone()));
        assert!(in_range(0x150, 0, r.clone()), "empty buffer inside");
        assert!(in_range(0x200, 0, r.clone()), "empty buffer at the end");
    }

    #[test]
    fn in_range_rejects_buffers_that_stick_out() {
        let r = 0x100..0x200;
        assert!(!in_range(0x0FF, 1, r.clone()), "starts below");
        assert!(!in_range(0x0FF, 2, r.clone()), "straddles the start");
        assert!(!in_range(0x1FF, 2, r.clone()), "straddles the end");
        assert!(!in_range(0x200, 1, r.clone()), "starts at the end");
        assert!(!in_range(0x100, 0x101, r.clone()), "longer than the region");
        assert!(!in_range(0, 0, r.clone()));
    }

    #[test]
    fn in_range_does_not_overflow() {
        let r = 0x100..0x200;
        assert!(!in_range(usize::MAX, 2, r.clone()));
        assert!(!in_range(0x180, usize::MAX, r.clone()));
        assert!(!in_range(usize::MAX, usize::MAX, r.clone()));
        let top = usize::MAX - 0xFF..usize::MAX;
        assert!(in_range(usize::MAX - 0x10, 0x10, top.clone()));
        assert!(!in_range(usize::MAX - 0x10, 0x11, top));
    }

    #[test]
    fn empty_region_accepts_nothing_but_empty_buffers_at_its_start() {
        assert!(!in_range(0x100, 1, 0x100..0x100));
        assert!(in_range(0x100, 0, 0x100..0x100));
        assert!(!in_range(0x101, 0, 0x100..0x100));
    }

    #[test]
    fn users_read_flash_and_user_ram_but_write_only_user_ram() {
        let _k = lock();
        set_flash_range(0x0000_0000..0x0010_0000);
        set_uram_range(0x2000_8000..0x2004_0000);
        let kram = 0x2000_0000;
        let periph = 0x4000_2000;

        assert!(user_can_read(0x0001_0000, 16));
        assert!(!user_can_write(0x0001_0000, 16), "flash is read-only");
        assert!(user_can_read(0x2000_8000, 16));
        assert!(user_can_write(0x2000_8000, 16));
        assert!(user_can_write(0x2003_FFF0, 16));
        for p in [kram, 0x2000_7FF0, periph, 0x6000_0000] {
            assert!(!user_can_read(p, 16), "{p:#x}");
            assert!(!user_can_write(p, 16), "{p:#x}");
        }
        assert!(!user_can_read(0x2000_7FF8, 16), "starts in kernel RAM");
        assert!(!user_can_write(0x2003_FFF8, 16), "runs off the end of RAM");
        assert!(!user_can_read(0x000F_FFF8, 16), "runs off the end of flash");
    }

    #[test]
    fn host_defaults_reject_every_user_pointer() {
        let _k = lock();
        let buf = [0u8; 16];
        assert!(!user_can_read(buf.as_ptr() as usize, buf.len()));
        assert!(!user_can_write(buf.as_ptr() as usize, buf.len()));
    }
}
