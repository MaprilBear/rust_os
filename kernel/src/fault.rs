//! What happens when a thread cannot continue: it reports a fault (the Fault
//! syscall, e.g. after a panic) or takes a MemManage fault.
//!
//! Normally both stop the system (there is no thread teardown yet). With the
//! `ktest` feature (on-target tests, docs/TESTING.md) the thread is *parked*
//! instead (blocked for good, in no wait queue), so the other tests go on, and
//! a thread may announce a MemManage fault it expects (`ExpectFault`): the
//! fault then writes a value the thread chose, with the fault's MMFSR in bits
//! 24-31, into a word of user RAM, which the test harness reads to see that
//! (and how) the fault happened.

#[cfg(feature = "ktest")]
use core::cell::Cell;

#[cfg(feature = "ktest")]
use crate::scheduler::{scheduler, Scheduler};
#[cfg(feature = "ktest")]
use crate::ThreadId;

/// An armed expected fault: the thread, and where to write what.
#[cfg(feature = "ktest")]
#[derive(Clone, Copy)]
struct Expected {
    thread: ThreadId,
    ptr: usize,
    value: u32,
}

#[cfg(feature = "ktest")]
static EXPECTED: critical_section::Mutex<Cell<Option<Expected>>> =
    critical_section::Mutex::new(Cell::new(None));

/// Block the running thread for good and switch away. Handler mode only.
#[cfg(feature = "ktest")]
pub(crate) fn park_current() {
    unsafe { scheduler().block() };
    crate::os_yield();
}

/// The ExpectFault syscall: arm the hook for the calling thread (`ptr` 0:
/// disarm). `ptr` must be a word-aligned word of user RAM. 0 ok, 1 error.
pub(crate) fn expect_fault(ptr: usize, value: usize) -> usize {
    #[cfg(feature = "ktest")]
    {
        let Some(thread) = (unsafe { scheduler().current_thread() }) else {
            return 1;
        };
        let armed = match ptr {
            0 => None,
            p if p % 4 == 0 && crate::syscall::user_can_write(p, 4) => Some(Expected {
                thread,
                ptr,
                value: value as u32,
            }),
            _ => return 1,
        };
        critical_section::with(|cs| EXPECTED.borrow(cs).set(armed));
        0
    }
    #[cfg(not(feature = "ktest"))]
    {
        let _ = (ptr, value);
        1
    }
}

/// Called by the MemManage handler for a fault in thread mode, with the
/// fault's MMFSR (CFSR bits 0-7). If the running thread expected it: write
/// its value (MMFSR in bits 24-31), disarm, park the thread and return true
/// (the handler then returns, and PendSV switches away). Otherwise false.
///
/// Never for a fault while the CPU stacked or unstacked the thread's frame
/// (MSTKERR, MUNSTKERR, MLSPERR): its stack pointer is then somewhere it may
/// not write, and PendSV would save registers there with privilege.
#[cfg_attr(not(target_os = "none"), allow(dead_code))] // host: called by test support
pub(crate) fn on_thread_memfault(mmfsr: u8) -> bool {
    const STACKING: u8 = (1 << 3) | (1 << 4) | (1 << 5);
    if mmfsr & STACKING != 0 {
        return false;
    }
    #[cfg(feature = "ktest")]
    {
        let current = unsafe { scheduler().current_thread() };
        let hit = critical_section::with(|cs| {
            let cell = EXPECTED.borrow(cs);
            match cell.get() {
                Some(e) if Some(e.thread) == current => {
                    cell.set(None);
                    Some(e)
                }
                _ => None,
            }
        });
        if let Some(e) = hit {
            // Checked by `expect_fault`: aligned, in user RAM.
            let word = e.value | (mmfsr as u32) << 24;
            unsafe { (e.ptr as *mut u32).write_volatile(word) };
            park_current();
            return true;
        }
        false
    }
    #[cfg(not(feature = "ktest"))]
    {
        let _ = mmfsr;
        false
    }
}

/// Test support: forget any armed expected fault.
#[cfg(all(not(target_os = "none"), any(test, feature = "test-support")))]
pub(crate) fn reset() {
    #[cfg(feature = "ktest")]
    critical_section::with(|cs| EXPECTED.borrow(cs).set(None));
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;
    #[cfg(feature = "ktest")]
    use crate::ThreadState;

    #[cfg(feature = "ktest")]
    const IACCVIOL: u8 = 1 << 0;
    #[cfg(feature = "ktest")]
    const DACCVIOL: u8 = 1 << 1;
    use userlib::Syscall::ExpectFault;

    /// A word of "user RAM" for the kernel to write to.
    struct Word(Box<u32>);
    use std::boxed::Box;

    impl Word {
        fn new() -> Self {
            let w = Word(Box::new(0));
            let p = w.addr();
            set_uram_range(p..p + 4);
            w
        }
        fn addr(&self) -> usize {
            &*self.0 as *const u32 as usize
        }
        #[cfg(feature = "ktest")]
        fn get(&self) -> u32 {
            unsafe { (self.addr() as *const u32).read_volatile() }
        }
    }

    fn arm(ptr: usize, value: usize) -> usize {
        dispatch2(ExpectFault as u8, ptr, value, Caller::Thread)
    }

    #[test]
    #[cfg(not(feature = "ktest"))]
    fn expect_fault_is_refused_without_the_ktest_feature() {
        let _k = lock();
        boot();
        context_switch();
        let w = Word::new();
        assert_eq!(arm(w.addr(), 1), 1);
    }

    #[test]
    #[cfg(feature = "ktest")]
    fn an_expected_fault_writes_the_value_and_parks_the_thread() {
        let _k = lock();
        boot();
        let t = spawn(1);
        context_switch();
        let w = Word::new();
        assert_eq!(arm(w.addr(), 0xF00D), 0);
        assert!(thread_memfault(DACCVIOL));
        assert_eq!(w.get(), 0xF00D | (DACCVIOL as u32) << 24, "value and MMFSR");
        assert_eq!(thread_state(t), Some(ThreadState::Blocked));
        assert!(pendsv_pending());
        assert_eq!(context_switch(), Some(0));
    }

    #[test]
    #[cfg(feature = "ktest")]
    fn the_hook_fires_once() {
        let _k = lock();
        boot();
        spawn(1);
        let t2 = spawn(1);
        context_switch();
        let w = Word::new();
        arm(w.addr(), 7);
        assert!(thread_memfault(IACCVIOL));
        assert_eq!(context_switch(), Some(t2));
        assert!(!thread_memfault(IACCVIOL), "disarmed after the first fault");
    }

    #[test]
    #[cfg(feature = "ktest")]
    fn a_fault_in_another_thread_is_not_the_expected_one() {
        let _k = lock();
        boot();
        let a = spawn(1);
        let b = spawn(1);
        assert_eq!(context_switch(), Some(a));
        let w = Word::new();
        arm(w.addr(), 7);
        userlib::sys_yield();
        assert_eq!(context_switch(), Some(b));
        assert!(!thread_memfault(DACCVIOL));
        assert_eq!(w.get(), 0, "nothing written");
        assert_eq!(thread_state(b), Some(ThreadState::Running));
    }

    #[test]
    #[cfg(feature = "ktest")]
    fn arming_with_zero_disarms() {
        let _k = lock();
        boot();
        spawn(1);
        context_switch();
        let w = Word::new();
        arm(w.addr(), 7);
        assert_eq!(arm(0, 0), 0);
        assert!(!thread_memfault(DACCVIOL));
        assert_eq!(w.get(), 0);
    }

    #[test]
    #[cfg(feature = "ktest")]
    fn the_word_must_be_aligned_user_ram() {
        let _k = lock();
        boot();
        spawn(1);
        context_switch();
        let w = Word::new();
        let other = Box::new(0u32); // not user RAM
        assert_eq!(arm(w.addr() + 1, 7), 1, "misaligned");
        assert_eq!(arm(w.addr() + 4, 7), 1, "past the end");
        assert_eq!(arm(&*other as *const u32 as usize, 7), 1, "not user RAM");
        assert!(!thread_memfault(DACCVIOL), "nothing armed");
    }

    /// A fault while stacking a frame means the thread's SP points where it
    /// may not write; parking would let PendSV save registers there.
    #[test]
    #[cfg(feature = "ktest")]
    fn stacking_faults_are_never_parked() {
        let _k = lock();
        boot();
        let t = spawn(1);
        context_switch();
        let w = Word::new();
        for stacking in [1 << 3, 1 << 4, 1 << 5] {
            arm(w.addr(), 7);
            assert!(!thread_memfault(DACCVIOL | stacking), "{stacking:#x}");
        }
        assert_eq!(w.get(), 0);
        assert_eq!(thread_state(t), Some(ThreadState::Running));
    }

    #[test]
    #[cfg(feature = "ktest")]
    fn boot_code_cannot_arm_it() {
        let _k = lock();
        let w = Word::new();
        assert_eq!(dispatch2(ExpectFault as u8, w.addr(), 7, Caller::Boot), 1);
    }
}
