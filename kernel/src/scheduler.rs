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
