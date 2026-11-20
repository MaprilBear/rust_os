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

#[cfg(all(not(target_os = "none"), any(test, feature = "test-support")))]
extern crate std;

#[cfg(all(target_os = "none", feature = "test-support"))]
compile_error!("the `test-support` feature is for host builds only");

#[cfg(all(feature = "debugcon", feature = "rtt"))]
compile_error!("choose one console: `debugcon` (Renode) or `rtt` (hardware)");

#[macro_use]
pub mod debugcon;

mod arch;
mod fault;
mod irq;
#[cfg(test)]
mod model_tests;
mod scheduler;
pub mod semaphore;
mod simple_list;
mod syscall;
#[cfg(all(not(target_os = "none"), any(test, feature = "test-support")))]
pub mod test_support;

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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

#[cfg(test)]
mod tests {
    use crate::scheduler::scheduler;
    use crate::test_support::*;
    use crate::ThreadState;

    /// The TCB fields `create_tcb` fills in, for thread `id`.
    fn stack_of(id: crate::ThreadId) -> (usize, usize, usize) {
        let tcb = unsafe { scheduler().tcb(id) }.expect("no such thread");
        (
            tcb.stack_base as usize,
            tcb.stack_layout.size(),
            tcb.sp as usize,
        )
    }

    #[test]
    fn threads_get_sequential_ids_and_start_ready() {
        let _k = lock();
        let a = spawn(3);
        let b = spawn(1);
        assert_eq!((a, b), (0, 1));
        assert_eq!(thread_state(a), Some(ThreadState::Ready));
        assert_eq!(thread_state(b), Some(ThreadState::Ready));
        let prio = |id| unsafe { scheduler().tcb(id) }.unwrap().priority;
        assert_eq!((prio(a), prio(b)), (3, 1));
    }

    #[test]
    fn stack_size_is_rounded_down_to_even_words_with_a_32_word_minimum() {
        let _k = lock();
        let cases = [(64, 64), (65, 64), (33, 32), (31, 32), (1, 32), (0, 32)];
        for (asked, words) in cases {
            let id = spawn_with(dummy_entry, asked, 1);
            let (base, size, _) = stack_of(id);
            assert_eq!(size, words * 4, "{asked} words asked");
            assert_eq!(base % 8, 0, "stack base is 8-byte aligned");
        }
    }

    #[test]
    fn saved_sp_points_at_the_initial_frame() {
        let _k = lock();
        // One cast, compared with itself: Rust (and Miri) do not promise that
        // two casts of the same function give the same address.
        let entry: crate::ThreadEntry = dummy_entry;
        let id = spawn_with(entry, 64, 1);
        let (base, size, sp) = stack_of(id);
        assert_eq!(sp, base + size - 16 * 4);
        assert_eq!((base + size) % 8, 0, "top of stack is 8-byte aligned");
        // The PC slot (second word from the top) holds the entry point.
        let pc = unsafe { *((base + size - 8) as *const u32) };
        assert_eq!(pc, (entry as usize as u32) | 1);
        // r0-r12 start at zero (the stack is zero-allocated).
        let regs = unsafe { core::slice::from_raw_parts(sp as *const u32, 13) };
        assert!(regs.iter().all(|&r| r == 0));
    }

    #[test]
    #[should_panic(expected = "thread entry is not in user text")]
    fn thread_entry_outside_user_text_is_rejected() {
        let _k = lock();
        let entry = dummy_entry as crate::ThreadEntry as usize;
        set_user_text_range(entry + 0x100..entry + 0x200);
        spawn(1);
    }

    #[test]
    fn user_text_check_ignores_the_thumb_bit() {
        let _k = lock();
        set_user_text_range(0x1000..0x2000);
        assert!(crate::is_user_text(0x1000));
        assert!(crate::is_user_text(0x1001));
        assert!(crate::is_user_text(0x1FFF), "0x1FFE with the Thumb bit");
        assert!(!crate::is_user_text(0x0FFF));
        assert!(!crate::is_user_text(0x2000));
        assert!(!crate::is_user_text(0x2001));
    }

    #[test]
    fn boot_adds_the_idle_thread_at_priority_7() {
        let _k = lock();
        boot();
        let tcb = unsafe { scheduler().tcb(0) }.expect("idle thread");
        assert_eq!(tcb.priority, 7);
        assert_eq!(tcb.stack_layout.size(), 1024 * 4);
        assert_eq!(tcb.state, ThreadState::Ready);
        assert_eq!(context_switch(), Some(0), "idle runs when nothing else can");
    }

    /// Known issue 13: priority 8+ panics on an out-of-bounds index in the
    /// policy, after the TCB was already added, so a half-made thread stays.
    #[test]
    #[ignore = "known issue 13: priority above 7 leaves a half-added thread"]
    fn priority_above_7_is_rejected_cleanly() {
        let _k = lock();
        let r = std::panic::catch_unwind(|| spawn(8));
        let msg = r.expect_err("priority 8 must be rejected");
        let msg = (msg.downcast_ref::<std::string::String>().cloned())
            .or_else(|| {
                msg.downcast_ref::<&str>()
                    .map(|s| std::string::String::from(*s))
            })
            .unwrap_or_default();
        assert!(msg.contains("priority"), "unclear message: {msg:?}");
        assert_eq!(thread_state(0), None, "no thread was added");
    }

    #[test]
    fn tick_requests_a_context_switch() {
        let _k = lock();
        assert!(!pendsv_pending());
        tick();
        assert!(pendsv_pending());
    }
}
