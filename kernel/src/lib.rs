#![allow(future_incompatible)]
// TODO: Support cooperative and premptive threads like Zephyr
// TODO: Implement workqueues
// TODO: Move kernel memory to a specific page
#![no_std]

//! The privileged kernel. link.ld places this crate (and only this crate) in the
//! KFLASH/KRAM regions, which the MPU makes privileged-only. Unprivileged threads
//! must never call into it directly: they use `userlib` (syscalls) instead.
//!
//! Boot is the reset handler in `arch/armv7m/reset.rs`, which calls the
//! application's `#[no_mangle] extern "C" fn user_main()` to register threads
//! and drivers.
//!
//! Everything CPU- or linker-specific is behind `arch` (see `arch/mod.rs`), so
//! the rest of the crate also builds for a host and runs in host tests.

#[macro_use]
pub mod debugcon;

mod arch;
mod irq;
mod scheduler;
pub mod semaphore;
mod simple_list;
mod syscall;

extern crate alloc;

pub use irq::{irq_attach, irq_enable, irq_set_priority, irq_unpend, Interrupt};
pub use semaphore::give_from_isr;

use core::alloc::Layout;

use crate::scheduler::{scheduler, Scheduler};

/// True if `addr` (a function address; the Thumb bit is ignored) lies in user
/// flash, i.e. code that unprivileged threads may execute.
fn is_user_text(addr: usize) -> bool {
    arch::user_text_range().contains(&(addr & !1))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Ready,
    Running,
    Blocked,
    Sleeping,
}

// PendSVHandler stores the saved SP at byte offset 4 (`str r2, [r1, #4]`), so
// `priority` then `sp` must stay the first two fields (checked at compile time
// in arch/armv7m/vectors.rs).
#[repr(C)]
pub struct TCB {
    priority: u8,
    sp: *mut u32,
    /// Base and layout of the stack allocation in the user heap.
    stack_base: *mut u8,
    stack_layout: Layout,
    state: ThreadState,
    id: ThreadId,
    sleep_ms: u32,
}

/// Identifies a thread (its slot in the scheduler's thread list).
pub type ThreadId = usize;

/// One SysTick period: advance sleep timers and request a reschedule.
#[cfg_attr(not(target_os = "none"), allow(dead_code))] // host: no SysTick
pub(crate) fn os_tick() {
    unsafe { scheduler().tick() };
    os_yield();
}

pub type ThreadEntry = extern "C" fn() -> !;
fn create_tcb(thread_entry: ThreadEntry, stack_words: usize, priority: u8) -> ThreadId {
    // Threads run unprivileged, so their code must be in user flash.
    assert!(
        is_user_text(thread_entry as usize),
        "thread entry is not in user text"
    );

    // Keep the top of the stack 8-byte aligned (AAPCS).
    let stack_words = (stack_words & !1).max(32);
    let layout = Layout::from_size_align(stack_words * 4, 8).unwrap();

    unsafe {
        // Zeroed, so the fabricated R0-R12 frame slots start at 0.
        let base = arch::alloc_user_stack(layout);
        assert!(!base.is_null(), "out of user memory for thread stack");

        // The allocation is zeroed, word-aligned and owned by this thread.
        let stack = core::slice::from_raw_parts_mut(base as *mut u32, stack_words);
        let sp = (base as *mut u32).add(arch::init_stack(stack, thread_entry as usize));

        scheduler().add_thread(TCB {
            id: 0,
            priority,
            sp,
            stack_base: base,
            stack_layout: layout,
            state: ThreadState::Ready,
            sleep_ms: 0,
        })
    }
}

/// Called once by the boot code, before `user_main`.
#[cfg_attr(not(target_os = "none"), allow(dead_code))] // host: no boot code
fn os_init() {
    arch::init_heaps();
    os_add_thread(userlib::idle_thread, 1024, 7);
}

/// Register a thread. Call only from `user_main` (privileged, before the
/// scheduler starts). `priority` 0 is highest, 7 is the idle thread's. Returns
/// the new thread's id.
pub fn os_add_thread(thread_entry: ThreadEntry, stack_size: usize, priority: u8) -> ThreadId {
    create_tcb(thread_entry, stack_size, priority)
}

/// Kernel-internal yield. Handler mode only (SysTick, SVC, ISRs): PendSV has
/// the lowest priority, so the switch happens on exception return. Threads
/// call `sys_yield`.
fn os_yield() {
    arch::pend_context_switch();
}

#[no_mangle]
extern "C" fn pick_next_thread() {
    unsafe { scheduler().schedule() }
}

pub fn os_get_frc() -> u32 {
    arch::frc()
}
