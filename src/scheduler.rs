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

pub struct RoundRobinScheduler {
    tcb_list: SimpleList<TCB>,
    sleep_list: SimpleList<ThreadId>,
}

pub static SCHEDULER: Global<RoundRobinScheduler> = Global::new(RoundRobinScheduler {
    tcb_list: SimpleList::new(),
    sleep_list: SimpleList::new(),
});

/// Safety: caller must not hold another reference to the scheduler.
pub unsafe fn scheduler() -> &'static mut RoundRobinScheduler {
    &mut *SCHEDULER.as_ptr()
}

impl Scheduler for RoundRobinScheduler {
    fn current_thread(&self) -> ThreadId {
        unsafe { (*CURR_TCB).id }
    }

    fn schedule(&mut self) {
        critical_section::with(|_| {
            let len = self.tcb_list.len();
            if len == 0 {
                return;
            }

            let start = unsafe {
                if CURR_TCB.is_null() {
                    0
                } else {
                    let current = &mut *CURR_TCB;
                    if current.state == ThreadState::Running {
                        current.state = ThreadState::Ready;
                    }
                    (current.id + 1) % len
                }
            };

            for offset in 0..len {
                let index = (start + offset) % len;
                if let Some(tcb) = self.tcb_list[index].as_mut() {
                    if tcb.state == ThreadState::Ready {
                        tcb.state = ThreadState::Running;
                        //dprintln!("Scheduled next thread: {:p}", tcb);
                        unsafe { NEXT_TCB = tcb as *mut TCB };
                        break;
                    }
                }
            }
        });
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

    fn add_thread(&mut self, thread: TCB) {
        critical_section::with(|_| {
            let index = self.tcb_list.push(thread);
            self.tcb_list[index].as_mut().unwrap().id = index;
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
