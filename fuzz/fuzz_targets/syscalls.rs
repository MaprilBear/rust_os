//! Hostile syscalls: arbitrary syscall numbers and arguments from the running
//! thread, interleaved with ticks, interrupt gives and context switches.
//!
//! The kernel must never panic (apart from `Fault`, which halts by design and
//! is left out) and these invariants must hold after every step:
//! at most one Running thread; with no switch pending, the current thread is
//! Running and no Ready thread outranks it; every Blocked thread waits on
//! exactly one semaphore; a semaphore has waiters exactly when its count is
//! negative, as many as the count says.
//!
//! The semaphores are an array the harness declares as the `.ksem` section
//! (`set_ksem_range`), so handles go through the kernel's real check (inside
//! the section, a multiple of the object size), not the host registry.
//!
//! Arguments (r0 and r1) are drawn from semaphore handles (valid, off by a
//! few bytes, or unregistered), pointers into a host buffer that stands in for
//! user RAM (`DebugWrite` must read only inside it), and raw integers.

#![no_main]

use arbitrary::Arbitrary;
use kernel::semaphore::{__handle_for, KSemaphore};
use kernel::test_support::*;
use kernel::ThreadState;
use libfuzzer_sys::fuzz_target;
use userlib::{Semaphore, Syscall};

/// Stands in for the `.ksem` section.
static OBJECTS: [KSemaphore; 3] = [KSemaphore::new(0), KSemaphore::new(1), KSemaphore::new(2)];
static SEMS: [Semaphore; 3] = [
    __handle_for(&OBJECTS[0]),
    __handle_for(&OBJECTS[1]),
    __handle_for(&OBJECTS[2]),
];
/// Outside the `.ksem` range: the kernel must reject its handle.
static STRANGER: Semaphore = kernel::semaphore!(1);

/// Stands in for user RAM, so pointer arguments can be valid.
static mut URAM: [u8; 4096] = [0; 4096];

#[derive(Arbitrary, Debug)]
enum Arg {
    Sem(u8),
    SemOffset(u8, i8),
    Stranger,
    UserBuf(u16),
    Raw(usize),
}

#[derive(Arbitrary, Debug)]
enum Op {
    /// Number, r0, r1.
    Syscall(u8, Arg, Arg),
    Tick,
    IsrGive(u8),
    Switch,
}

#[derive(Arbitrary, Debug)]
struct Input {
    priorities: Vec<u8>,
    ops: Vec<Op>,
}

fn arg(a: &Arg) -> usize {
    match *a {
        Arg::Sem(i) => SEMS[i as usize % SEMS.len()].handle(),
        Arg::SemOffset(i, d) => SEMS[i as usize % SEMS.len()]
            .handle()
            .wrapping_add_signed(d as isize),
        Arg::Stranger => STRANGER.handle(),
        Arg::UserBuf(off) => (&raw const URAM) as usize + off as usize % 4096,
        Arg::Raw(v) => v,
    }
}

fn check_semaphores(threads: usize) {
    let mut queued = vec![0; threads];
    for (i, s) in SEMS.iter().enumerate() {
        let (count, waiters) = semaphore_state(s);
        let expected = if count < 0 { count.unsigned_abs() } else { 0 };
        assert_eq!(
            waiters.len(),
            expected,
            "semaphore {i}: count {count}, waiters {waiters:?}"
        );
        for w in waiters {
            queued[w] += 1;
        }
    }
    for (t, &q) in queued.iter().enumerate() {
        let blocked = thread_state(t) == Some(ThreadState::Blocked);
        assert_eq!(
            q,
            usize::from(blocked),
            "thread {t}: blocked {blocked}, in {q} wait queues"
        );
    }
}

fn check_invariants(threads: usize) {
    check_semaphores(threads);
    let running: Vec<_> = (0..threads)
        .filter(|&t| thread_state(t) == Some(ThreadState::Running))
        .collect();
    assert!(running.len() <= 1, "several Running threads: {running:?}");
    if !pendsv_pending() {
        let cur = current().expect("no current thread");
        assert_eq!(running, [cur], "current thread is not the Running one");
        let me = thread_priority(cur).unwrap();
        for t in 0..threads {
            if thread_state(t) == Some(ThreadState::Ready) {
                assert!(
                    thread_priority(t).unwrap() >= me,
                    "Ready thread {t} outranks running thread {cur}"
                );
            }
        }
    }
}

fuzz_target!(|input: Input| {
    let _k = lock();
    for s in &SEMS {
        reset_semaphore(s);
    }
    let ksem = OBJECTS.as_ptr() as usize;
    set_ksem_range(ksem..ksem + core::mem::size_of_val(&OBJECTS));
    let uram = (&raw const URAM) as usize;
    set_uram_range(uram..uram + 4096);

    boot();
    for p in input.priorities.iter().take(6) {
        spawn(p % 8); // 7: at idle's level
    }
    let threads = 1 + input.priorities.len().min(6);
    dispatch(Syscall::Start as u8, 0, Caller::Boot);
    context_switch();
    check_invariants(threads);

    for op in input.ops.iter().take(256) {
        match op {
            Op::Syscall(num, a0, a1) => {
                // A thread only runs once PendSV has; idle makes no syscalls;
                // Fault halts the kernel by design.
                run_pending_switch();
                if current() == Some(0) || *num == Syscall::Fault as u8 {
                    continue;
                }
                dispatch2(*num, arg(a0), arg(a1), Caller::Thread);
            }
            Op::Tick => tick(),
            Op::IsrGive(i) => in_isr(|| kernel::give_from_isr(&SEMS[*i as usize % SEMS.len()])),
            Op::Switch => {
                run_pending_switch();
            }
        }
        check_invariants(threads);
        take_log();
    }
});
