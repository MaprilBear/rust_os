use alloc::vec::Vec;

use crate::{simple_list::SimpleList, ThreadId, ThreadState, TCB};
use core::ptr;
use userlib::Global;

// Globals used to identify the current and next threads for thr context switch logic
#[no_mangle]
pub static mut CURR_TCB: *mut TCB = ptr::null_mut();

#[no_mangle]
pub static mut NEXT_TCB: *mut TCB = ptr::null_mut();

pub trait Scheduler {
    fn schedule(&mut self);
    /// Add a thread and return the id it was given.
    fn add_thread(&mut self, thread: TCB) -> ThreadId;
    #[allow(dead_code)]
    fn remove_thread(&mut self, thread: ThreadId);
    fn tick(&mut self);
    /// Put the running thread to sleep. False if there is no running thread.
    fn sleep(&mut self, ms: u32) -> bool;
    /// Mark the running thread blocked (no-op if there is none).
    fn block(&mut self);
    fn unblock(&mut self, thread: ThreadId);
    /// The running thread; None before the scheduler starts (boot code).
    fn current_thread(&self) -> Option<ThreadId>;
}

pub trait SchedulePolicy {
    /// Choose the next thread to run. Return an index into `threads`.
    /// `current` is the thread that was just running (if any).
    fn pick_next(
        &mut self,
        threads: &SimpleList<TCB>,
        current: Option<ThreadId>,
    ) -> Option<ThreadId>;

    // Optional hooks: variants override only what they need.
    fn on_add(&mut self, _thread: &TCB) {}
    #[allow(dead_code)]
    fn on_remove(&mut self, _id: ThreadId) {}
}

pub struct SchedulerCore<P: SchedulePolicy> {
    tcb_list: SimpleList<TCB>,
    sleep_list: SimpleList<ThreadId>,
    policy: P,
}

impl<P: SchedulePolicy> SchedulerCore<P> {
    pub const fn new(policy: P) -> Self {
        Self {
            tcb_list: SimpleList::new(),
            sleep_list: SimpleList::new(),
            policy,
        }
    }
}

impl<P: SchedulePolicy> Scheduler for SchedulerCore<P> {
    fn schedule(&mut self) {
        critical_section::with(|_| {
            let current = unsafe {
                if CURR_TCB.is_null() {
                    None
                } else {
                    let c = &mut *CURR_TCB;
                    if c.state == ThreadState::Running {
                        c.state = ThreadState::Ready;
                    }
                    Some(c.id)
                }
            };

            if let Some(id) = self.policy.pick_next(&self.tcb_list, current) {
                if let Some(tcb) = self.tcb_list[id].as_mut() {
                    tcb.state = ThreadState::Running;
                    unsafe { NEXT_TCB = tcb as *mut TCB };
                }
            } else {
                unsafe { NEXT_TCB = CURR_TCB };
            }
        });
    }

    fn add_thread(&mut self, thread: TCB) -> ThreadId {
        critical_section::with(|_| {
            let index = self.tcb_list.push(thread);
            self.tcb_list[index].as_mut().unwrap().id = index;
            self.policy.on_add(self.tcb_list[index].as_ref().unwrap());
            index
        })
    }

    fn current_thread(&self) -> Option<ThreadId> {
        unsafe { CURR_TCB.as_ref().map(|tcb| tcb.id) }
    }

    fn tick(&mut self) {
        critical_section::with(|_| {
            self.sleep_list.retain(|tcb_id| {
                if let Some(tcb) = self.tcb_list[*tcb_id].as_mut() {
                    if tcb.sleep_ms > 0 {
                        tcb.sleep_ms -= 1;
                    }
                    if tcb.sleep_ms == 0 {
                        tcb.state = ThreadState::Ready;
                        false
                    } else {
                        true
                    }
                } else {
                    false
                }
            });
        });
    }

    fn sleep(&mut self, ms: u32) -> bool {
        critical_section::with(|_| {
            let Some(current) = (unsafe { CURR_TCB.as_mut() }) else {
                return false;
            };
            current.sleep_ms = ms;
            current.state = ThreadState::Sleeping;
            let current_id = current.id;
            self.sleep_list.push(current_id);
            true
        })
    }

    fn remove_thread(&mut self, thread: ThreadId) {
        critical_section::with(|_| {
            let tcb_list = &mut self.tcb_list;
            let index = {
                tcb_list
                    .iter()
                    .position(|slot| slot.as_ref().map(|tcb| tcb.id == thread).unwrap_or(false))
            };
            if let Some(index) = index {
                tcb_list.remove(index);
            }
            self.policy.on_remove(thread);
        });
    }

    fn block(&mut self) {
        critical_section::with(|_| {
            if let Some(current) = unsafe { CURR_TCB.as_mut() } {
                current.state = ThreadState::Blocked;
            }
        });
    }

    fn unblock(&mut self, thread: ThreadId) {
        critical_section::with(|_| {
            if let Some(tcb) = self.tcb_list[thread].as_mut() {
                tcb.state = ThreadState::Ready;
            }
        });
    }
}

#[allow(dead_code)]
pub struct RoundRobinPolicy;

impl SchedulePolicy for RoundRobinPolicy {
    fn pick_next(
        &mut self,
        threads: &SimpleList<TCB>,
        current: Option<ThreadId>,
    ) -> Option<ThreadId> {
        let len = threads.slot_len();
        if len == 0 {
            return None;
        }
        let start = current.map_or(0, |id| (id + 1) % len);
        (0..len)
            .map(|o| (start + o) % len)
            .find(|&i| matches!(threads[i].as_ref(), Some(t) if t.state == ThreadState::Ready))
    }
}

const PRIORITY_LEVELS: usize = 8;
pub struct PriorityPolicy {
    priority_lists: [Vec<ThreadId>; PRIORITY_LEVELS],
}

impl PriorityPolicy {
    pub const fn new() -> Self {
        Self {
            priority_lists: [const { Vec::new() }; PRIORITY_LEVELS],
        }
    }
}

impl SchedulePolicy for PriorityPolicy {
    fn on_add(&mut self, thread: &TCB) {
        self.priority_lists[thread.priority as usize].push(thread.id);
    }

    fn on_remove(&mut self, id: ThreadId) {
        for list in &mut self.priority_lists {
            if let Some(index) = list.iter().position(|&thread_id| thread_id == id) {
                list.remove(index);
                break;
            }
        }
    }

    fn pick_next(
        &mut self,
        threads: &SimpleList<TCB>,
        current: Option<ThreadId>,
    ) -> Option<ThreadId> {
        for list in self.priority_lists.iter() {
            if !list.is_empty() {
                let len = list.len();
                let start = current
                    .and_then(|id| list.iter().position(|&thread_id| thread_id == id))
                    .map_or(0, |index| (index + 1) % len);
                if let Some(index) = (0..len).map(|o| (start + o) % len).find(|&i| {
                    matches!(
                        threads[list[i]].as_ref(),
                        Some(t) if t.state == ThreadState::Ready
                    )
                }) {
                    return Some(list[index]);
                }
            }
        }
        None
    }
}

pub type ActiveScheduler = SchedulerCore<PriorityPolicy>;

pub static SCHEDULER: Global<ActiveScheduler> =
    Global::new(SchedulerCore::new(PriorityPolicy::new()));

/// Safety: caller must not hold another reference to the scheduler.
pub unsafe fn scheduler() -> &'static mut ActiveScheduler {
    &mut *SCHEDULER.as_ptr()
}

/// Test support: look up a thread.
#[cfg(all(not(target_os = "none"), any(test, feature = "test-support")))]
impl<P: SchedulePolicy> SchedulerCore<P> {
    pub(crate) fn tcb(&self, id: ThreadId) -> Option<&TCB> {
        self.tcb_list.iter().nth(id).and_then(|slot| slot.as_ref())
    }
}

/// Test support: back to "no threads", freeing every thread's stack (on a
/// host, stacks come from the global allocator).
#[cfg(all(not(target_os = "none"), any(test, feature = "test-support")))]
pub(crate) fn reset() {
    critical_section::with(|_| unsafe {
        let old = core::mem::replace(
            &mut *SCHEDULER.as_ptr(),
            SchedulerCore::new(PriorityPolicy::new()),
        );
        for tcb in old.tcb_list.iter().flatten() {
            alloc::alloc::dealloc(tcb.stack_base, tcb.stack_layout);
        }
        CURR_TCB = ptr::null_mut();
        NEXT_TCB = ptr::null_mut();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;
    use core::alloc::Layout;
    use userlib::{sys_sleep, sys_yield, Semaphore};

    static SEM: Semaphore = crate::semaphore!(0);

    /// Make `id` the running thread (it must be the one the policy picks).
    fn run(id: ThreadId) {
        assert_eq!(context_switch(), Some(id));
        assert_eq!(thread_state(id), Some(ThreadState::Running));
    }

    // Priority policy (the active one), through the global scheduler ////////

    #[test]
    fn no_threads_means_nothing_to_run() {
        let _k = lock();
        assert_eq!(context_switch(), None);
        assert_eq!(unsafe { scheduler().current_thread() }, None);
    }

    #[test]
    fn highest_priority_ready_thread_runs() {
        let _k = lock();
        boot();
        let low = spawn(5);
        let high = spawn(2);
        let mid = spawn(3);
        run(high);
        assert_eq!(thread_state(low), Some(ThreadState::Ready));
        assert_eq!(thread_state(mid), Some(ThreadState::Ready));
    }

    #[test]
    fn a_running_thread_that_yields_alone_at_its_priority_keeps_running() {
        let _k = lock();
        boot();
        let t = spawn(1);
        let _low = spawn(2);
        run(t);
        sys_yield();
        assert!(pendsv_pending());
        run(t);
    }

    #[test]
    fn equal_priorities_round_robin_on_yield() {
        let _k = lock();
        boot();
        let [a, b, c] = [spawn(2), spawn(2), spawn(2)];
        run(a);
        for expected in [b, c, a, b] {
            sys_yield();
            run(expected);
        }
    }

    #[test]
    fn round_robin_skips_threads_that_are_not_ready() {
        let _k = lock();
        boot();
        let [a, b, c] = [spawn(2), spawn(2), spawn(2)];
        run(a);
        sys_yield();
        run(b);
        sys_sleep(10);
        run(c);
        sys_yield();
        run(a);
        sys_yield();
        run(c);
    }

    #[test]
    fn lower_priority_never_runs_while_a_higher_one_is_ready() {
        let _k = lock();
        boot();
        let high = spawn(0);
        let low = spawn(1);
        run(high);
        for _ in 0..5 {
            tick();
            run(high);
            sys_yield();
            run(high);
        }
        assert_eq!(thread_state(low), Some(ThreadState::Ready));
    }

    #[test]
    fn blocked_and_sleeping_threads_fall_back_to_lower_priorities() {
        let _k = lock();
        register_semaphore(&SEM);
        boot();
        let a = spawn(0);
        let b = spawn(1);
        let c = spawn(2);
        run(a);
        SEM.take(); // blocks a
        assert_eq!(thread_state(a), Some(ThreadState::Blocked));
        run(b);
        sys_sleep(3);
        assert_eq!(thread_state(b), Some(ThreadState::Sleeping));
        run(c);
        sys_sleep(3);
        run(0); // only idle is left
    }

    #[test]
    #[cfg_attr(miri, ignore = "known issue 9: Miri reports UB in CURR_TCB/NEXT_TCB")]
    fn idle_runs_when_nothing_else_is_ready_and_is_preempted_by_a_wakeup() {
        let _k = lock();
        boot();
        let t = spawn(1);
        run(t);
        sys_sleep(1);
        run(0);
        tick();
        assert_eq!(thread_state(t), Some(ThreadState::Ready));
        assert!(pendsv_pending());
        run(t);
        assert_eq!(thread_state(0), Some(ThreadState::Ready));
    }

    /// Known issue 11: when the previous thread ran at another priority, the
    /// scan of a level restarts at its first thread. A higher-priority thread
    /// that wakes every tick then hands the CPU back to the same
    /// lower-priority thread every time, and its peers starve.
    #[test]
    #[ignore = "known issue 11: round robin restarts at the first thread after a preemption"]
    fn preemption_does_not_starve_threads_at_the_same_level() {
        let _k = lock();
        boot();
        let high = spawn(1);
        let peers = [spawn(2), spawn(2), spawn(2)];
        let mut ran = std::vec::Vec::new();
        run(high);
        for _ in 0..6 {
            sys_sleep(1); // high sleeps one tick...
            ran.push(context_switch().unwrap()); // ...a peer runs...
            tick(); // ...and is preempted when high wakes.
            run(high);
        }
        for p in peers {
            assert!(ran.contains(&p), "thread {p} never ran: {ran:?}");
        }
    }

    // Sleep and tick ////////////////////////////////////////////////////////

    #[test]
    fn sleeping_thread_wakes_after_exactly_its_ticks() {
        let _k = lock();
        boot();
        let t = spawn(1);
        run(t);
        sys_sleep(3);
        assert_eq!(thread_state(t), Some(ThreadState::Sleeping));
        assert!(pendsv_pending(), "sleep yields");
        for _ in 0..2 {
            tick();
            assert_eq!(thread_state(t), Some(ThreadState::Sleeping));
        }
        tick();
        assert_eq!(thread_state(t), Some(ThreadState::Ready));
    }

    #[test]
    fn sleep_zero_wakes_on_the_next_tick() {
        let _k = lock();
        boot();
        let t = spawn(1);
        run(t);
        sys_sleep(0);
        assert_eq!(thread_state(t), Some(ThreadState::Sleeping));
        run(0);
        tick();
        assert_eq!(thread_state(t), Some(ThreadState::Ready));
    }

    #[test]
    fn several_sleepers_wake_independently() {
        let _k = lock();
        boot();
        let a = spawn(1);
        let b = spawn(2);
        let c = spawn(3);
        run(a);
        sys_sleep(1);
        run(b);
        sys_sleep(3);
        run(c);
        sys_sleep(2);
        let states = || [a, b, c].map(|t| thread_state(t).unwrap());
        use ThreadState::{Ready, Sleeping};
        tick();
        assert_eq!(states(), [Ready, Sleeping, Sleeping]);
        tick();
        assert_eq!(states(), [Ready, Sleeping, Ready]);
        tick();
        assert_eq!(states(), [Ready, Ready, Ready]);
        tick();
        assert_eq!(states(), [Ready, Ready, Ready], "nothing left to wake");
    }

    #[test]
    #[cfg_attr(miri, ignore = "known issue 9: Miri reports UB in CURR_TCB/NEXT_TCB")]
    fn a_woken_thread_runs_again_and_can_sleep_again() {
        let _k = lock();
        boot();
        let t = spawn(1);
        for _ in 0..3 {
            run(t);
            sys_sleep(2);
            run(0);
            tick();
            tick();
        }
        run(t);
    }

    #[test]
    fn sleep_with_no_running_thread_fails() {
        let _k = lock();
        assert!(!unsafe { scheduler().sleep(5) });
    }

    // Block and unblock ////////////////////////////////////////////////////

    #[test]
    #[cfg_attr(miri, ignore = "known issue 9: Miri reports UB in CURR_TCB/NEXT_TCB")]
    fn block_and_unblock_change_the_state() {
        let _k = lock();
        boot();
        let t = spawn(1);
        run(t);
        unsafe { scheduler().block() };
        assert_eq!(thread_state(t), Some(ThreadState::Blocked));
        run(0);
        unsafe { scheduler().unblock(t) };
        assert_eq!(thread_state(t), Some(ThreadState::Ready));
        run(t);
    }

    #[test]
    fn block_with_no_running_thread_does_nothing() {
        let _k = lock();
        let t = spawn(1);
        unsafe { scheduler().block() };
        assert_eq!(thread_state(t), Some(ThreadState::Ready));
    }

    // Remove ////////////////////////////////////////////////////////////////

    #[test]
    #[cfg_attr(miri, ignore = "known issue 9: Miri reports UB in CURR_TCB/NEXT_TCB")]
    fn removed_thread_is_never_scheduled_and_its_slot_is_reused() {
        let _k = lock();
        boot();
        let a = spawn(1);
        let b = spawn(1);
        run(a);
        unsafe { scheduler().remove_thread(b) };
        assert_eq!(thread_state(b), None);
        for _ in 0..3 {
            sys_yield();
            run(a);
        }
        assert_eq!(spawn(1), b, "the lowest free slot is reused");
        // The policy forgot the old b: the new one gets one turn, not two.
        for expected in [b, a, b, a] {
            sys_yield();
            run(expected);
        }
    }

    #[test]
    fn removing_an_unknown_thread_does_nothing() {
        let _k = lock();
        boot();
        let t = spawn(1);
        unsafe { scheduler().remove_thread(42) };
        run(t);
    }

    #[test]
    #[cfg_attr(miri, ignore = "known issue 2: remove_thread leaks the thread's stack")]
    fn tick_drops_sleep_entries_of_removed_threads() {
        let _k = lock();
        boot();
        let t = spawn(1);
        run(t);
        sys_sleep(5);
        run(0);
        unsafe { scheduler().remove_thread(t) };
        tick(); // must not touch the empty slot
        assert_eq!(thread_state(t), None);
        assert_eq!(unsafe { scheduler().sleep_list.len() }, 0, "entry dropped");
    }

    #[test]
    fn tick_leaves_blocked_threads_alone() {
        let _k = lock();
        boot();
        let t = spawn(1);
        run(t);
        unsafe { scheduler().block() };
        run(0);
        for _ in 0..3 {
            tick();
        }
        assert_eq!(thread_state(t), Some(ThreadState::Blocked));
    }

    /// Known issue 2: a sleep-list entry outlives its thread; the next thread
    /// given the same id is "woken" by the old entry.
    #[test]
    #[ignore = "known issue 2: remove_thread leaves the id in the sleep list"]
    fn stale_sleep_entry_does_not_wake_a_reused_id() {
        let _k = lock();
        register_semaphore(&SEM);
        boot();
        let old = spawn(1);
        run(old);
        sys_sleep(5);
        run(0); // not removing the running thread (that is the next test)
        unsafe { scheduler().remove_thread(old) };
        let new = spawn(1);
        assert_eq!(new, old, "id reused");
        run(new);
        SEM.take(); // blocks
        tick();
        assert_eq!(thread_state(new), Some(ThreadState::Blocked));
    }

    /// Known issue 2: a semaphore wait-queue entry outlives its thread.
    #[test]
    #[ignore = "known issue 2: remove_thread leaves the id in semaphore wait queues"]
    fn stale_semaphore_waiter_does_not_wake_a_reused_id() {
        let _k = lock();
        register_semaphore(&SEM);
        boot();
        let old = spawn(1);
        run(old);
        SEM.take(); // blocks, queued as a waiter
        run(0);
        unsafe { scheduler().remove_thread(old) };
        let new = spawn(1);
        assert_eq!(new, old, "id reused");
        run(new);
        sys_sleep(10);
        run(0);
        // A give from an ISR finds the stale waiter.
        in_isr(|| crate::give_from_isr(&SEM));
        assert_eq!(thread_state(new), Some(ThreadState::Sleeping));
    }

    /// Known issue 2: removing the running thread leaves CURR_TCB pointing at
    /// the emptied slot. Reading through it is undefined behaviour (the slot
    /// is now `None`); today it returns a garbage id.
    #[test]
    #[ignore = "known issue 2: removing the running thread leaves CURR_TCB dangling"]
    fn removing_the_running_thread_clears_the_current_thread() {
        let _k = lock();
        boot();
        let t = spawn(1);
        run(t);
        unsafe { scheduler().remove_thread(t) };
        assert_eq!(unsafe { scheduler().current_thread() }, None);
    }

    // Policies on their own /////////////////////////////////////////////////

    fn tcb(id: ThreadId, priority: u8, state: ThreadState) -> TCB {
        TCB {
            priority,
            sp: ptr::null_mut(),
            stack_base: ptr::null_mut(),
            stack_layout: Layout::new::<u32>(),
            state,
            id,
            sleep_ms: 0,
        }
    }

    fn list(states: &[Option<ThreadState>]) -> SimpleList<TCB> {
        let mut l = SimpleList::new();
        for i in 0..states.len() {
            l.push(tcb(i, 0, ThreadState::Ready));
        }
        for (i, s) in states.iter().enumerate() {
            match s {
                Some(s) => l[i].as_mut().unwrap().state = *s,
                None => l.remove(i),
            }
        }
        l
    }

    #[test]
    fn round_robin_policy_picks_the_next_ready_slot_after_current() {
        use ThreadState::*;
        let threads = list(&[
            Some(Ready),
            Some(Blocked),
            None,
            Some(Ready),
            Some(Sleeping),
        ]);
        let mut p = RoundRobinPolicy;
        assert_eq!(p.pick_next(&threads, None), Some(0));
        assert_eq!(p.pick_next(&threads, Some(0)), Some(3));
        assert_eq!(p.pick_next(&threads, Some(3)), Some(0), "wraps");
        assert_eq!(p.pick_next(&threads, Some(1)), Some(3));
    }

    #[test]
    fn round_robin_policy_re_picks_current_if_it_is_the_only_ready_one() {
        use ThreadState::*;
        let threads = list(&[Some(Blocked), Some(Ready), Some(Sleeping)]);
        assert_eq!(RoundRobinPolicy.pick_next(&threads, Some(1)), Some(1));
    }

    #[test]
    fn round_robin_policy_with_nothing_ready_picks_nothing() {
        use ThreadState::*;
        let mut p = RoundRobinPolicy;
        assert_eq!(p.pick_next(&SimpleList::new(), None), None);
        let threads = list(&[Some(Blocked), None, Some(Sleeping), Some(Running)]);
        assert_eq!(p.pick_next(&threads, Some(3)), None);
    }

    #[test]
    fn priority_policy_on_its_own() {
        let mut threads = SimpleList::new();
        let mut p = PriorityPolicy::new();
        for (id, prio) in [(0, 4), (1, 2), (2, 4), (3, 2)] {
            threads.push(tcb(id, prio, ThreadState::Ready));
            p.on_add(threads[id].as_ref().unwrap());
        }
        assert_eq!(p.pick_next(&threads, None), Some(1));
        assert_eq!(p.pick_next(&threads, Some(1)), Some(3));
        assert_eq!(p.pick_next(&threads, Some(3)), Some(1));
        threads[1].as_mut().unwrap().state = ThreadState::Blocked;
        threads[3].as_mut().unwrap().state = ThreadState::Blocked;
        assert_eq!(p.pick_next(&threads, Some(3)), Some(0));
        assert_eq!(p.pick_next(&threads, Some(0)), Some(2));
        p.on_remove(2);
        assert_eq!(p.pick_next(&threads, Some(0)), Some(0));
        p.on_remove(0);
        assert_eq!(p.pick_next(&threads, None), None);
    }
}
