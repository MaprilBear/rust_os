//! Model-based tests: random sequences of thread syscalls, ticks, interrupt
//! gives and context switches run against the real kernel and, side by side,
//! a small reference model of what the kernel should do. After every step the
//! two must agree, and the scheduler invariants must hold:
//!
//! * at most one thread is Running; with no switch pending, exactly one, and
//!   it is the current thread;
//! * with no switch pending, no Ready thread has a higher priority than the
//!   running one (a switch picks a highest-priority Ready thread);
//! * every thread keeps the state the model says (none is lost or woken
//!   early or late);
//! * semaphore counts and wait queues match the model (counts are conserved,
//!   waiters wake in FIFO order: the kernel's rule today; known issue 19
//!   asks for the highest-priority waiter first, and the model's `wake`
//!   changes with that fix).
//!
//! The model does not predict *which* of several equal-priority Ready threads
//! runs next: it checks the kernel's choice is a valid one and adopts it. It
//! does check one policy-free fairness rule: a thread that yields while a
//! Ready peer at its priority exists gives the CPU to a peer. (The wider
//! fairness bug is known issue 11's test.)
//!
//! Only sequences the hardware can produce are generated. Interrupts (tick,
//! `give_from_isr`) can arrive at any time, and the idle thread makes no
//! syscalls. A thread makes a syscall only when it is running: not once its
//! own syscall or a tick has requested a switch (PendSV runs before thread
//! mode resumes). An interrupt above SVCall's priority can still pend a
//! switch between a thread's `svc` and the SVC handler (late arrival), so a
//! switch requested by `give_from_isr` still lets one syscall through.

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use std::format;

use proptest::prelude::*;
use userlib::Semaphore;

use crate::test_support::*;
use crate::{ThreadId, ThreadState};

static SEMS: [Semaphore; 3] = [
    crate::semaphore!(0),
    crate::semaphore!(1),
    crate::semaphore!(2),
];
const INITIAL: [isize; 3] = [0, 1, 2];

#[derive(Clone, Copy, Debug)]
enum Op {
    /// Thread syscalls, made by the running thread.
    Yield,
    Sleep(u32),
    Take(usize),
    Give(usize),
    /// Interrupts.
    Tick,
    IsrGive(usize),
    /// PendSV (only if a switch is pending).
    Switch,
}

fn op() -> impl Strategy<Value = Op> {
    let sem = 0..SEMS.len();
    prop_oneof![
        2 => Just(Op::Yield),
        3 => (0u32..4).prop_map(Op::Sleep),
        4 => sem.clone().prop_map(Op::Take),
        3 => sem.clone().prop_map(Op::Give),
        4 => Just(Op::Tick),
        2 => sem.prop_map(Op::IsrGive),
        6 => Just(Op::Switch),
    ]
}

#[derive(Clone, Debug)]
struct Thread {
    priority: u8,
    state: ThreadState,
    /// Ticks left while Sleeping.
    sleep: u32,
}

#[derive(Debug)]
struct Model {
    threads: Vec<Thread>,
    current: Option<ThreadId>,
    pending: bool,
    /// Whether the running thread can make a syscall now (see the module
    /// docs).
    may_syscall: bool,
    /// The thread whose Yield requested the pending switch.
    yielder: Option<ThreadId>,
    sems: Vec<(isize, VecDeque<ThreadId>)>,
}

impl Model {
    fn new(priorities: &[u8]) -> Self {
        // Thread 0 is idle (priority 7), as boot() creates it.
        let threads = core::iter::once(7)
            .chain(priorities.iter().copied())
            .map(|priority| Thread {
                priority,
                state: ThreadState::Ready,
                sleep: 0,
            })
            .collect();
        Model {
            threads,
            current: None,
            pending: false,
            may_syscall: false,
            yielder: None,
            sems: INITIAL.iter().map(|&c| (c, VecDeque::new())).collect(),
        }
    }

    fn cur(&mut self) -> &mut Thread {
        let id = self.current.expect("no running thread");
        &mut self.threads[id]
    }

    fn wake(&mut self, sem: usize) {
        self.sems[sem].0 += 1;
        if let Some(w) = self.sems[sem].1.pop_front() {
            self.threads[w].state = ThreadState::Ready;
            self.pending = true;
        }
    }

    /// Apply `op` to the model; false if it cannot happen now (skipped).
    fn apply(&mut self, op: Op) -> bool {
        let thread_op = matches!(op, Op::Yield | Op::Sleep(_) | Op::Take(_) | Op::Give(_));
        if thread_op && (!self.may_syscall || self.current == Some(0)) {
            return false;
        }
        match op {
            Op::Yield => {
                self.pending = true;
                self.yielder = self.current;
            }
            Op::Sleep(ms) => {
                let t = self.cur();
                t.state = ThreadState::Sleeping;
                t.sleep = ms;
                self.pending = true;
            }
            Op::Take(s) => {
                self.sems[s].0 -= 1;
                if self.sems[s].0 < 0 {
                    let id = self.current.unwrap();
                    self.sems[s].1.push_back(id);
                    self.cur().state = ThreadState::Blocked;
                    self.pending = true;
                }
            }
            Op::Give(s) | Op::IsrGive(s) => self.wake(s),
            Op::Tick => {
                for t in &mut self.threads {
                    if t.state == ThreadState::Sleeping {
                        t.sleep = t.sleep.saturating_sub(1);
                        if t.sleep == 0 {
                            t.state = ThreadState::Ready;
                        }
                    }
                }
                self.pending = true;
                // SysTick shares SVCall's priority: no syscall slips in.
                self.may_syscall = false;
            }
            Op::Switch => {
                if !self.pending {
                    return false;
                }
                // Checked against the kernel's choice in `switch`.
            }
        }
        if thread_op && self.pending {
            self.may_syscall = false;
        }
        true
    }

    /// The kernel switched to `chosen`: check it was a valid choice and
    /// adopt it.
    fn switch(&mut self, chosen: Option<ThreadId>) -> Result<(), TestCaseError> {
        self.pending = false;
        if let Some(c) = self.current {
            if self.threads[c].state == ThreadState::Running {
                self.threads[c].state = ThreadState::Ready;
            }
        }
        let best = self
            .threads
            .iter()
            .filter(|t| t.state == ThreadState::Ready)
            .map(|t| t.priority)
            .min();
        let chosen = chosen.ok_or_else(|| TestCaseError::fail("no thread chosen"))?;
        let t = &self.threads[chosen];
        prop_assert_eq!(
            t.state,
            ThreadState::Ready,
            "switched to thread {} which is not Ready",
            chosen
        );
        prop_assert_eq!(
            Some(t.priority),
            best,
            "thread {} is not a highest-priority Ready thread",
            chosen
        );
        if let Some(y) = self.yielder.take() {
            let peer_ready = self.threads.iter().enumerate().any(|(id, p)| {
                id != y && p.state == ThreadState::Ready && p.priority == self.threads[y].priority
            });
            if peer_ready && t.priority == self.threads[y].priority {
                prop_assert_ne!(
                    chosen,
                    y,
                    "thread {} yielded but a Ready peer did not run",
                    y
                );
            }
        }
        self.threads[chosen].state = ThreadState::Running;
        self.current = Some(chosen);
        self.may_syscall = true;
        Ok(())
    }
}

/// Run `op` on the kernel (the model already accepted it).
fn run_on_kernel(op: Op) {
    match op {
        Op::Yield => userlib::sys_yield(),
        Op::Sleep(ms) => userlib::sys_sleep(ms),
        Op::Take(s) => assert_eq!(userlib::sys_sem_take(SEMS[s].handle()), 0),
        Op::Give(s) => assert_eq!(userlib::sys_sem_give(SEMS[s].handle()), 0),
        Op::Tick => tick(),
        Op::IsrGive(s) => in_isr(|| crate::give_from_isr(&SEMS[s])),
        Op::Switch => unreachable!("handled by the caller"),
    }
}

fn check(model: &Model, step: usize) -> Result<(), TestCaseError> {
    let at = |what: &str| format!("step {step}: {what}");
    prop_assert_eq!(current(), model.current, "{}", at("current thread"));
    prop_assert_eq!(pendsv_pending(), model.pending, "{}", at("switch pending"));
    for (id, t) in model.threads.iter().enumerate() {
        prop_assert_eq!(
            thread_state(id),
            Some(t.state),
            "{}",
            at(&format!("thread {id}"))
        );
    }
    for (i, sem) in SEMS.iter().enumerate() {
        let (count, waiters) = semaphore_state(sem);
        prop_assert_eq!(count, model.sems[i].0, "{}", at(&format!("sem {i} count")));
        prop_assert!(
            waiters.iter().eq(model.sems[i].1.iter()),
            "{}: waiters {:?}, model {:?}",
            at(&format!("sem {i}")),
            waiters,
            model.sems[i].1
        );
    }
    // Invariants, stated directly on the kernel (not only via the model).
    let running: Vec<ThreadId> = (0..model.threads.len())
        .filter(|&id| thread_state(id) == Some(ThreadState::Running))
        .collect();
    prop_assert!(
        running.len() <= 1,
        "{}: running {:?}",
        at("one Running"),
        running
    );
    if !pendsv_pending() {
        prop_assert_eq!(
            &running[..],
            &[current().unwrap()][..],
            "{}",
            at("current Running")
        );
        let me = thread_priority(current().unwrap()).unwrap();
        for id in 0..model.threads.len() {
            if thread_state(id) == Some(ThreadState::Ready) {
                prop_assert!(
                    thread_priority(id).unwrap() >= me,
                    "{}: Ready thread {} outranks the running one",
                    at("priority"),
                    id
                );
            }
        }
    }
    Ok(())
}

fn run(priorities: &[u8], ops: &[Op]) -> Result<(), TestCaseError> {
    let _k = lock();
    for s in &SEMS {
        register_semaphore(s);
    }
    boot();
    for &p in priorities {
        spawn(p);
    }
    let mut model = Model::new(priorities);
    // Start, as the boot code does.
    dispatch(userlib::Syscall::Start as u8, 0, Caller::Boot);
    model.pending = true;
    model.switch(context_switch())?;
    check(&model, 0)?;

    for (step, &op) in ops.iter().enumerate() {
        if !model.apply(op) {
            continue;
        }
        if let Op::Switch = op {
            model.switch(context_switch())?;
        } else {
            run_on_kernel(op);
        }
        check(&model, step + 1)?;
    }
    Ok(())
}

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: if cfg!(miri) { 8 } else { 512 },
        // Miri runs isolated (no file system).
        failure_persistence: if cfg!(miri) {
            None
        } else {
            ProptestConfig::default().failure_persistence
        },
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn kernel_matches_the_model(
        // 7 too: threads at idle's level.
        priorities in proptest::collection::vec(0u8..8, 1..6),
        ops in proptest::collection::vec(op(), 1..120),
    ) {
        run(&priorities, &ops)?;
    }

    /// All threads at one priority: exercises round robin and wait queues
    /// with many peers.
    #[test]
    fn kernel_matches_the_model_with_equal_priorities(
        n in 1usize..6,
        ops in proptest::collection::vec(op(), 1..120),
    ) {
        run(&std::vec![3; n], &ops)?;
    }
}
