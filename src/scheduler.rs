use crate::{global::Global, simple_list::SimpleList, ThreadId, ThreadState, TCB};
use core::ptr;

// Globals used to identify the current and next threads for thr context switch logic
#[no_mangle]
pub static mut CURR_TCB: *mut TCB = ptr::null_mut();

#[no_mangle]
pub static mut NEXT_TCB: *mut TCB = ptr::null_mut();

pub trait Scheduler {
    fn schedule(&mut self);
    fn add_thread(&mut self, thread: TCB);
    fn remove_thread(&mut self, thread: ThreadId);
    fn tick(&mut self);
    fn sleep(&mut self, ms: u32);
    fn block(&mut self);
    fn unblock(&mut self, thread: ThreadId);
    fn current_thread(&self) -> ThreadId;
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

    fn add_thread(&mut self, thread: TCB) {
        critical_section::with(|_| {
            let index = self.tcb_list.push(thread);
            self.tcb_list[index].as_mut().unwrap().id = index;
            self.policy.on_add(self.tcb_list[index].as_ref().unwrap());
        });
    }

    fn current_thread(&self) -> ThreadId {
        unsafe { (*CURR_TCB).id }
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

    fn sleep(&mut self, ms: u32) {
        critical_section::with(|_| {
            let current = unsafe { &mut *CURR_TCB };
            current.sleep_ms = ms;
            current.state = ThreadState::Sleeping;
            let current_id = current.id;
            self.sleep_list.push(current_id);
        });
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
            let current = unsafe { &mut *CURR_TCB };
            current.state = ThreadState::Blocked;
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

pub struct RoundRobin;

impl SchedulePolicy for RoundRobin {
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

pub type ActiveScheduler = SchedulerCore<RoundRobin>;

pub static SCHEDULER: Global<ActiveScheduler> = Global::new(SchedulerCore::new(RoundRobin));

/// Safety: caller must not hold another reference to the scheduler.
pub unsafe fn scheduler() -> &'static mut ActiveScheduler {
    &mut *SCHEDULER.as_ptr()
}
