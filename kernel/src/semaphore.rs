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
//! ```ignore
//! static TX_DONE: userlib::Semaphore = kernel::semaphore!(0);
//! pub static UARTE0: Uarte = Uarte::new(BASE, semaphore!(1), semaphore!(0));
//! ```
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

use crate::scheduler::{scheduler, Scheduler};
use crate::{arch, os_yield, ThreadId};

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
    /// The count it was created with, so host test support can reset it.
    #[cfg(all(not(target_os = "none"), any(test, feature = "test-support")))]
    initial: isize,
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
            #[cfg(all(not(target_os = "none"), any(test, feature = "test-support")))]
            initial: count,
        }
    }
}

/// Declares the kernel object as a static in kernel RAM and evaluates to the
/// user handle for it. Usable in `static` and `const` initialisers.
#[macro_export]
macro_rules! semaphore {
    ($count:expr) => {{
        #[cfg_attr(target_os = "none", link_section = ".ksem")]
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
/// (On a host build `.ksem` is empty, so every handle is rejected unless host
/// test support registered it or set a range.)
fn lookup(handle: usize) -> Option<&'static KSemaphore> {
    let range = arch::ksem_range();
    let size = size_of::<KSemaphore>();
    let in_ksem = range.contains(&handle) && (handle - range.start).is_multiple_of(size);
    #[cfg(all(not(target_os = "none"), any(test, feature = "test-support")))]
    let in_ksem = in_ksem || arch::host::testing::ksem_registered(handle);
    if in_ksem {
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

/// Test support: put the semaphore behind `handle` back in its initial state
/// (initial count, no waiters).
///
/// # Safety
/// `handle` must come from a `userlib::Semaphore` created by `semaphore!`.
#[cfg(all(not(target_os = "none"), any(test, feature = "test-support")))]
pub(crate) unsafe fn reset_object(handle: usize) {
    let sem = &*(handle as *const KSemaphore);
    critical_section::with(|_| {
        let st = &mut *sem.state.get();
        st.count = sem.initial;
        st.waiters.clear();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;
    use crate::ThreadState;
    use std::vec::Vec;
    use userlib::Syscall::{SemGive, SemTake};
    use userlib::{sys_sem_give, sys_sem_take, Semaphore};

    static ZERO: Semaphore = crate::semaphore!(0);
    static ONE: Semaphore = crate::semaphore!(1);
    static THREE: Semaphore = crate::semaphore!(3);

    fn state(sem: &Semaphore) -> &KSemState {
        // Safety: handles from `semaphore!` point at live objects; the test
        // holds the kernel lock, so nothing mutates it concurrently.
        unsafe { &*(*(sem.handle() as *const KSemaphore)).state.get() }
    }
    fn count(sem: &Semaphore) -> isize {
        state(sem).count
    }
    fn waiters(sem: &Semaphore) -> Vec<ThreadId> {
        state(sem).waiters.iter().copied().collect()
    }

    fn run(id: ThreadId) {
        assert_eq!(context_switch(), Some(id));
    }

    /// Boot, register `sems`, and run a thread at priority 1.
    fn setup(sems: &[&Semaphore]) -> ThreadId {
        for s in sems {
            register_semaphore(s);
        }
        boot();
        let t = spawn(1);
        run(t);
        t
    }

    #[test]
    fn take_of_an_available_unit_does_not_block() {
        let _k = lock();
        let t = setup(&[&ONE]);
        assert_eq!(sys_sem_take(ONE.handle()), 0);
        assert_eq!(count(&ONE), 0);
        assert_eq!(thread_state(t), Some(ThreadState::Running));
        assert!(!pendsv_pending());
    }

    #[test]
    fn counting_semaphore_hands_out_its_initial_count_then_blocks() {
        let _k = lock();
        let t = setup(&[&THREE]);
        for left in [2, 1, 0] {
            THREE.take();
            assert_eq!(count(&THREE), left);
            assert_eq!(thread_state(t), Some(ThreadState::Running));
        }
        THREE.take();
        assert_eq!(thread_state(t), Some(ThreadState::Blocked));
        assert_eq!(count(&THREE), -1);
    }

    #[test]
    fn take_at_zero_blocks_queues_and_requests_a_switch() {
        let _k = lock();
        let t = setup(&[&ZERO]);
        assert_eq!(sys_sem_take(ZERO.handle()), 0);
        assert_eq!(thread_state(t), Some(ThreadState::Blocked));
        assert_eq!(waiters(&ZERO), [t]);
        assert!(pendsv_pending());
        run(0); // idle
    }

    #[test]
    fn give_without_waiters_increments_without_a_switch() {
        let _k = lock();
        setup(&[&ZERO]);
        for n in 1..=3 {
            assert_eq!(sys_sem_give(ZERO.handle()), 0);
            assert_eq!(count(&ZERO), n, "no upper bound on the count");
        }
        assert!(!pendsv_pending());
    }

    #[test]
    fn equal_priority_waiters_wake_in_fifo_order() {
        let _k = lock();
        register_semaphore(&ZERO);
        boot();
        let ts = [spawn(1), spawn(1), spawn(1)];
        for &t in &ts {
            run(t);
            ZERO.take();
        }
        assert_eq!(waiters(&ZERO), ts);
        let mut woken = Vec::new();
        for _ in 0..3 {
            in_isr(|| give_from_isr(&ZERO));
            let now_ready: Vec<_> = ts
                .iter()
                .copied()
                .filter(|&t| thread_state(t) == Some(ThreadState::Ready) && !woken.contains(&t))
                .collect();
            assert_eq!(now_ready.len(), 1, "one give wakes one waiter");
            woken.push(now_ready[0]);
        }
        assert_eq!(woken, ts);
        assert_eq!(count(&ZERO), 0);
    }

    /// Known issue 19: `give` wakes the oldest waiter; it should wake the
    /// highest-priority one (FIFO among equal priorities).
    #[test]
    #[ignore = "known issue 19: give wakes the oldest waiter, not the highest-priority one"]
    fn the_highest_priority_waiter_wakes_first() {
        let _k = lock();
        register_semaphore(&ZERO);
        boot();
        let low = spawn(3);
        let high = spawn(1);
        let mid = spawn(2);
        // Make them block lowest priority first, so FIFO and priority differ.
        run(high);
        userlib::sys_sleep(2);
        run(mid);
        userlib::sys_sleep(1);
        run(low);
        ZERO.take();
        run(0);
        tick(); // mid wakes
        run(mid);
        ZERO.take();
        run(0);
        tick(); // high wakes
        run(high);
        ZERO.take();
        run(0);
        assert_eq!(waiters(&ZERO), [low, mid, high]);
        in_isr(|| give_from_isr(&ZERO));
        assert_eq!(thread_state(high), Some(ThreadState::Ready));
        assert_eq!(thread_state(low), Some(ThreadState::Blocked));
    }

    #[test]
    fn give_hands_the_unit_to_the_waiter() {
        let _k = lock();
        register_semaphore(&ONE);
        boot();
        let a = spawn(1);
        let b = spawn(2);
        let c = spawn(3);
        run(a);
        ONE.take(); // a holds it
        sys_sleep_and_run_next(b);
        ONE.take(); // b waits
        run(c); // a sleeps, b blocked: c runs
        assert_eq!(count(&ONE), -1);
        tick(); // wake a
        run(a);
        ONE.give(); // hands the unit to b
        assert_eq!(thread_state(b), Some(ThreadState::Ready));
        assert_eq!(count(&ONE), 0, "the unit went to b, not back to the count");
        run(a); // a is higher priority than b
        ONE.take(); // so a must wait now
        assert_eq!(thread_state(a), Some(ThreadState::Blocked));
        run(b);
    }

    /// The running thread sleeps one tick and `next` runs.
    fn sys_sleep_and_run_next(next: ThreadId) {
        userlib::sys_sleep(1);
        run(next);
    }

    #[test]
    fn giving_to_a_higher_priority_waiter_preempts_the_giver() {
        let _k = lock();
        register_semaphore(&ZERO);
        boot();
        let high = spawn(1);
        let low = spawn(2);
        run(high);
        ZERO.take();
        run(low);
        ZERO.give();
        assert!(pendsv_pending());
        run(high);
        assert_eq!(thread_state(low), Some(ThreadState::Ready));
    }

    #[test]
    fn giving_to_a_lower_priority_waiter_keeps_the_giver_running() {
        let _k = lock();
        register_semaphore(&ZERO);
        boot();
        let high = spawn(1);
        let low = spawn(2);
        run(high);
        userlib::sys_sleep(1);
        run(low);
        ZERO.take();
        run(0);
        tick();
        run(high);
        ZERO.give();
        assert_eq!(thread_state(low), Some(ThreadState::Ready));
        run(high);
    }

    #[test]
    fn with_takes_runs_and_gives() {
        let _k = lock();
        setup(&[&ONE]);
        let mut ran = false;
        ONE.with(|| {
            assert_eq!(count(&ONE), 0);
            ran = true;
        });
        assert!(ran);
        assert_eq!(count(&ONE), 1);
    }

    // Handle validation ////////////////////////////////////////////////////

    #[test]
    fn unregistered_handle_is_rejected() {
        let _k = lock();
        setup(&[]);
        assert_eq!(sys_sem_take(ONE.handle()), 1);
        assert_eq!(sys_sem_give(ONE.handle()), 1);
        for bogus in [0, 1, usize::MAX, &ONE as *const _ as usize] {
            assert_eq!(sys_sem_take(bogus), 1, "{bogus:#x}");
            assert_eq!(sys_sem_give(bogus), 1, "{bogus:#x}");
        }
        assert!(!pendsv_pending());
    }

    #[test]
    #[should_panic(expected = "bad semaphore handle")]
    fn semaphore_take_panics_on_a_bad_handle() {
        let _k = lock();
        setup(&[]);
        ONE.take();
    }

    static OBJECTS: [KSemaphore; 3] = [const { KSemaphore::new(1) }; 3];

    #[test]
    fn handles_inside_the_ksem_range_must_be_whole_objects() {
        let _k = lock();
        setup(&[]);
        let start = OBJECTS.as_ptr() as usize;
        let size = size_of::<KSemaphore>();
        set_ksem_range(start..start + 3 * size);
        for i in 0..3 {
            unsafe { reset_object(start + i * size) };
        }
        assert_eq!(sys_sem_take(start), 0);
        assert_eq!(sys_sem_take(start + 2 * size), 0);
        assert_eq!(sys_sem_give(start + size), 0);
        for bad in [start - size, start + 1, start + size - 1, start + 3 * size] {
            assert_eq!(sys_sem_take(bad), 1, "{:#x}", bad - start);
            assert_eq!(sys_sem_give(bad), 1, "{:#x}", bad - start);
        }
    }

    // Callers other than a running thread //////////////////////////////////

    #[test]
    fn boot_code_may_take_an_available_unit_but_not_block() {
        let _k = lock();
        register_semaphore(&ONE);
        let take = SemTake as u8;
        assert_eq!(dispatch(take, ONE.handle(), Caller::Boot), 0);
        assert_eq!(count(&ONE), 0);
        assert_eq!(dispatch(take, ONE.handle(), Caller::Boot), 1);
        assert_eq!(count(&ONE), 0, "a failed take changes nothing");
        assert!(waiters(&ONE).is_empty());
    }
    #[test]
    fn boot_code_may_give_and_wake_a_waiter() {
        let _k = lock();
        let t = setup(&[&ZERO]);
        ZERO.take();
        assert_eq!(dispatch(SemGive as u8, ZERO.handle(), Caller::Boot), 0);
        assert_eq!(thread_state(t), Some(ThreadState::Ready));
        assert!(pendsv_pending());
    }

    #[test]
    fn give_from_isr_wakes_a_waiter_and_requests_a_switch() {
        let _k = lock();
        let t = setup(&[&ZERO]);
        ZERO.take();
        run(0);
        in_isr(|| give_from_isr(&ZERO));
        assert_eq!(thread_state(t), Some(ThreadState::Ready));
        assert!(pendsv_pending());
        run(t);
    }

    #[test]
    fn give_from_isr_without_waiters_does_not_request_a_switch() {
        let _k = lock();
        setup(&[&ZERO]);
        in_isr(|| give_from_isr(&ZERO));
        assert_eq!(count(&ZERO), 1);
        assert!(!pendsv_pending());
    }

    #[test]
    #[should_panic(expected = "bad semaphore handle")]
    fn give_from_isr_panics_on_a_bad_handle() {
        let _k = lock();
        setup(&[]);
        in_isr(|| give_from_isr(&ZERO));
    }

    #[test]
    #[should_panic(expected = "Semaphore::give from an ISR")]
    fn semaphore_give_from_an_isr_panics() {
        let _k = lock();
        setup(&[&ZERO]);
        in_isr(|| ZERO.give());
    }

    #[test]
    fn register_semaphore_resets_count_and_waiters() {
        let _k = lock();
        let t = setup(&[&ONE]);
        ONE.take();
        ONE.take();
        assert_eq!(waiters(&ONE), [t]);
        register_semaphore(&ONE);
        assert_eq!(count(&ONE), 1);
        assert!(waiters(&ONE).is_empty());
    }
}
