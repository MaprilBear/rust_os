//! Host-side test support: cfg(test), or the `test-support` feature for other
//! crates' tests such as `drivers`. Not available on the target.
//!
//! # Rules
//!
//! The kernel keeps its state in globals, exactly as on hardware, so a test
//! that touches it must hold the kernel for its whole duration:
//!
//! ```ignore
//! let _k = kernel::test_support::lock();
//! ```
//!
//! `lock()` serialises the test against other kernel tests in the same test
//! binary and resets the kernel to "nothing booted, no threads": scheduler and
//! thread stacks, the running thread, the IRQ table, the Start flag, the fake
//! port (ranges, NVIC, PendSV, log) and the userlib hooks. Dropping the guard
//! uninstalls the syscall bridge, so a test that forgot `lock()` fails loudly
//! instead of using another test's kernel.
//!
//! Not reset: objects created by `semaphore!` (they are separate statics) and
//! driver statics. Register every semaphore a test uses with
//! [`register_semaphore`], which also puts it back in its initial state;
//! unregistered handles are rejected, as an invalid handle would be on
//! hardware.
//!
//! `lock()` is not reentrant (a helper must not call it while the test holds
//! it), and must not be called from inside `critical_section::with`.
//!
//! # Simulation model
//!
//! * There is no real thread switching. [`context_switch`] does what PendSV
//!   does (pick the next thread, make it current) and returns the new current
//!   thread. [`run_pending_switch`] does so only if a switch was requested.
//! * `userlib::sys_*` calls are bridged to the kernel's syscall dispatcher as
//!   calls from the current thread; there must be one ([`spawn`] then
//!   [`context_switch`]). A call that would block returns at once and leaves
//!   the thread `Blocked`, as the hardware thread would be at its `svc`. Calls
//!   from inside [`in_isr`] panic: on hardware they escalate to HardFault.
//! * [`dispatch`] calls the dispatcher directly, as any caller, with no checks.
//! * [`tick`] is one SysTick; [`in_isr`] and [`raise_irq`] run code as an
//!   interrupt handler.
//! * Lower priority numbers run first; [`boot`] adds the idle thread
//!   (priority 7) as the real boot code does.

use core::ops::Range;
use std::string::String;
use std::sync::{Mutex, MutexGuard};
use std::thread::{self, ThreadId as OsThreadId};

use crate::arch::host::testing as port;
use crate::scheduler::{self, scheduler, CURR_TCB, NEXT_TCB};
use crate::{irq, semaphore, syscall, ThreadId, ThreadState};

pub use crate::irq::Interrupt;
pub use crate::syscall::Caller;

static KERNEL_TEST_LOCK: Mutex<()> = Mutex::new(());
/// The test thread that holds `KERNEL_TEST_LOCK`.
static OWNER: Mutex<Option<OsThreadId>> = Mutex::new(None);

/// Exclusive access to the (global) kernel for one test. See the module docs.
pub struct KernelGuard {
    _lock: MutexGuard<'static, ()>,
}

impl Drop for KernelGuard {
    fn drop(&mut self) {
        userlib::host::set_syscall_handler(None);
        userlib::host::set_in_handler_mode(false);
        *OWNER.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

/// Lock and reset the kernel. A test that panicked while holding the lock does
/// not poison it for the others.
pub fn lock() -> KernelGuard {
    let guard = KERNEL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    port::reset();
    scheduler::reset();
    irq::reset();
    syscall::reset();
    userlib::host::set_in_handler_mode(false);
    userlib::host::set_syscall_handler(Some(syscall_bridge));
    *OWNER.lock().unwrap_or_else(|e| e.into_inner()) = Some(thread::current().id());
    KernelGuard { _lock: guard }
}

fn assert_owner() {
    let owner = *OWNER.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(
        owner,
        Some(thread::current().id()),
        "the kernel was used by a test that does not hold test_support::lock()"
    );
}

fn syscall_bridge(num: u8, arg0: usize) -> usize {
    assert_owner();
    assert!(
        !userlib::in_handler_mode(),
        "syscall {num} from an interrupt handler: on hardware this escalates to HardFault \
         (ISRs use kernel::give_from_isr)"
    );
    assert!(
        current().is_some(),
        "syscall {num} with no running thread: spawn() a thread and context_switch() first"
    );
    syscall::dispatch(num, arg0, Caller::Thread)
}

/// Call the syscall dispatcher directly as `caller`, without the bridge's
/// checks (e.g. to test what the kernel does with a syscall and no running
/// thread).
pub fn dispatch(num: u8, arg0: usize, caller: Caller) -> usize {
    assert_owner();
    syscall::dispatch(num, arg0, caller)
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Threads and scheduling
///////////////////////////////////////////////////////////////////////////////////////////////////

/// Do what the boot code does before `user_main`: add the idle thread.
pub fn boot() {
    assert_owner();
    crate::os_init();
}

/// A thread entry for tests. Never executed on the host.
pub extern "C" fn dummy_entry() -> ! {
    unreachable!("host tests never run thread code")
}

/// Create a thread (as `os_add_thread` does) and return its id.
pub fn spawn(priority: u8) -> ThreadId {
    spawn_with(dummy_entry, 64, priority)
}

/// `spawn` with an explicit entry and stack size (in words).
pub fn spawn_with(entry: crate::ThreadEntry, stack_words: usize, priority: u8) -> ThreadId {
    assert_owner();
    crate::os_add_thread(entry, stack_words, priority)
}

/// Do what PendSV does: clear the pending request, choose the next thread and
/// make it current. Returns the new current thread (None if there is none).
pub fn context_switch() -> Option<ThreadId> {
    assert_owner();
    port::clear_pendsv();
    crate::pick_next_thread();
    unsafe { CURR_TCB = NEXT_TCB };
    current()
}

/// `context_switch()` if one was requested (PendSV pending). Returns whether a
/// switch ran.
pub fn run_pending_switch() -> bool {
    let pending = port::pendsv_pending();
    if pending {
        context_switch();
    }
    pending
}

/// The running thread, if any.
pub fn current() -> Option<ThreadId> {
    unsafe { CURR_TCB.as_ref().map(|t| t.id) }
}

/// State of thread `id`, or None if there is no such thread.
pub fn thread_state(id: ThreadId) -> Option<ThreadState> {
    unsafe { scheduler().tcb(id).map(|t| t.state) }
}

/// One SysTick period (as the SysTick handler does).
pub fn tick() {
    assert_owner();
    crate::os_tick();
}

/// Run `f` as an interrupt handler. Nests, and restores the previous mode even
/// if `f` panics.
pub fn in_isr<R>(f: impl FnOnce() -> R) -> R {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            userlib::host::set_in_handler_mode(self.0);
        }
    }
    let _restore = Restore(userlib::in_handler_mode());
    userlib::host::set_in_handler_mode(true);
    f()
}

/// Run the handler attached to `irq` (with `irq_attach`) as the CPU would when
/// the interrupt fires. Panics if none is attached.
pub fn raise_irq(irq: Interrupt) {
    assert_owner();
    let handler = irq::handler_for(irq as usize).expect("no handler attached to this IRQ");
    in_isr(|| handler());
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Semaphores
///////////////////////////////////////////////////////////////////////////////////////////////////

/// Let the kernel accept `sem`'s handle for the rest of this test and put the
/// semaphore back in its initial state (initial count, no waiters).
pub fn register_semaphore(sem: &userlib::Semaphore) {
    assert_owner();
    // Safety: a `userlib::Semaphore` is created by `semaphore!`, so its handle
    // is the address of a live `KSemaphore`.
    unsafe { semaphore::reset_object(sem.handle()) };
    port::register_ksem(sem.handle());
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Fake port
///////////////////////////////////////////////////////////////////////////////////////////////////

/// Whether a context switch has been requested and not yet run.
pub fn pendsv_pending() -> bool {
    port::pendsv_pending()
}

/// Whether the Start syscall started the tick.
pub fn systick_started() -> bool {
    port::systick_started()
}

pub fn nvic_is_enabled(irq: Interrupt) -> bool {
    port::nvic_is_enabled(irq)
}

pub fn nvic_priority(irq: Interrupt) -> u8 {
    port::nvic_priority(irq)
}

pub fn nvic_unpend_count(irq: Interrupt) -> usize {
    port::nvic_unpend_count(irq)
}

/// Everything the kernel printed (`dprint!`) since the last call (capped at
/// 64 KiB).
pub fn take_log() -> String {
    String::from_utf8_lossy(&port::take_log()).into_owned()
}

/// Treat only `r` as user flash (thread entries and IRQ handlers outside it are
/// rejected). Default: all addresses.
pub fn set_user_text_range(r: Range<usize>) {
    port::set_user_text_range(r)
}

/// Treat `r` as the `.ksem` section, as on hardware (handles inside it at a
/// multiple of the object size are valid). Default: empty.
pub fn set_ksem_range(r: Range<usize>) {
    port::set_ksem_range(r)
}

/// Which memory counts as flash / user RAM for checks on pointers passed to
/// syscalls (no syscall takes a pointer yet). Default: none.
pub fn set_flash_range(r: Range<usize>) {
    port::set_flash_range(r)
}

pub fn set_uram_range(r: Range<usize>) {
    port::set_uram_range(r)
}

/// Tests of the test support itself: if these break, kernel tests can pass
/// for the wrong reason.
#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{catch_unwind, AssertUnwindSafe};

    static SEM: userlib::Semaphore = crate::semaphore!(1);

    extern "C" fn handler() {}

    #[test]
    fn lock_resets_threads_ranges_and_the_port() {
        {
            let _k = lock();
            boot();
            spawn(1);
            context_switch();
            userlib::sys_yield();
            register_semaphore(&SEM);
            set_ksem_range(0x10..0x20);
            set_flash_range(0x0..0x1000);
            set_uram_range(0x2000..0x3000);
            crate::irq_attach(Interrupt::RTC0, handler);
            crate::irq_enable(Interrupt::RTC0);
            crate::irq_set_priority(Interrupt::RTC0, 0x40);
            crate::irq_unpend(Interrupt::RTC0);
            set_user_text_range(0x100..0x200);
            dispatch(userlib::Syscall::Start as u8, 0, Caller::Boot);
            crate::dprint!("x");
        }
        let _k = lock();
        assert_eq!(current(), None);
        assert_eq!(thread_state(0), None);
        assert!(!pendsv_pending());
        assert!(!systick_started());
        assert_eq!(take_log(), "");
        assert_eq!(crate::arch::ksem_range(), 0..0);
        assert_eq!(crate::arch::user_text_range(), 0..usize::MAX);
        assert_eq!(crate::arch::flash_range(), 0..0);
        assert_eq!(crate::arch::uram_range(), 0..0);
        assert!(!nvic_is_enabled(Interrupt::RTC0));
        assert_eq!(nvic_priority(Interrupt::RTC0), 0);
        assert_eq!(nvic_unpend_count(Interrupt::RTC0), 0);
        assert!(crate::irq::handler_for(Interrupt::RTC0 as usize).is_none());
        assert!(
            matches!(
                crate::semaphore::ksem_take(SEM.handle()),
                crate::semaphore::TakeResult::Error
            ),
            "registered handles are forgotten"
        );
        assert_eq!(
            dispatch(userlib::Syscall::Start as u8, 0, Caller::Boot),
            0,
            "Start allowed again"
        );
    }

    #[test]
    #[should_panic(expected = "no running thread")]
    fn syscall_without_a_running_thread_panics() {
        let _k = lock();
        boot();
        userlib::sys_yield();
    }

    #[test]
    #[should_panic(expected = "from an interrupt handler")]
    fn syscall_from_an_isr_panics() {
        let _k = lock();
        boot();
        context_switch();
        in_isr(userlib::sys_yield);
    }

    #[test]
    #[should_panic(expected = "no host syscall handler installed")]
    fn syscall_after_the_guard_is_dropped_panics() {
        drop(lock());
        // Hold the raw mutex (not a guard) so no other test can install the
        // bridge meanwhile. The last guard dropped must have uninstalled it.
        let _raw = KERNEL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        userlib::sys_yield();
    }

    #[test]
    fn in_isr_nests_and_restores_thread_mode_even_on_panic() {
        let _k = lock();
        in_isr(|| {
            in_isr(|| assert!(userlib::in_handler_mode()));
            assert!(userlib::in_handler_mode(), "still in the outer handler");
        });
        assert!(!userlib::in_handler_mode());
        let r = catch_unwind(AssertUnwindSafe(|| in_isr(|| panic!("boom"))));
        assert!(r.is_err());
        assert!(!userlib::in_handler_mode());
    }

    #[test]
    fn run_pending_switch_runs_only_when_requested() {
        let _k = lock();
        boot();
        assert!(!run_pending_switch());
        assert_eq!(current(), None);
        tick();
        assert!(run_pending_switch());
        assert_eq!(current(), Some(0));
        assert!(!pendsv_pending());
    }

    #[test]
    fn a_panicking_test_does_not_poison_the_lock() {
        let r = std::thread::spawn(|| {
            let _k = lock();
            panic!("test failure while holding the kernel");
        })
        .join();
        assert!(r.is_err());
        let _k = lock();
        boot();
        assert_eq!(context_switch(), Some(0));
    }

    #[test]
    #[should_panic(expected = "does not hold test_support::lock()")]
    fn using_the_kernel_without_the_lock_panics() {
        let _k = lock();
        std::thread::spawn(boot)
            .join()
            .map_err(|e| std::panic::resume_unwind(e))
            .unwrap();
    }
}
