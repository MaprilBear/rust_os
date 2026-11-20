#![allow(future_incompatible)]
// TODO: Support cooperative and premptive threads like Zephyr
// TODO: Implement workqueues
// TODO: Move kernel memory to a specific page
#![no_std]

//! The privileged kernel. link.ld places this crate (and only this crate) in the
//! KFLASH/KRAM regions, which the MPU makes privileged-only. Unprivileged threads
//! must never call into it directly: they use `userlib` (syscalls) instead.
//!
//! Boot is `reset::ResetHandler`, which calls the application's
//! `#[no_mangle] extern "C" fn user_main()` to register threads and drivers.

#[macro_use]
pub mod debugcon;

mod reset;
mod scheduler;
pub mod semaphore;
mod simple_list;
mod syscall;
mod vectors;

extern crate alloc;

pub use semaphore::give_from_isr;
pub use vectors::{irq_attach, irq_enable, irq_set_priority, irq_unpend, Interrupt};

use vectors::core_periph;

use bitfields::bitflag;
use core::alloc::{GlobalAlloc, Layout};
use core::arch::asm;
use core::ptr::addr_of;
use embedded_alloc::TlsfHeap as Heap;

use crate::scheduler::{scheduler, Scheduler};

/// Kernel heap. Backed by a static in `.bss`, so it lives in KRAM (privileged only).
#[global_allocator]
static HEAP: Heap = Heap::empty();

/// User heap. Only thread stacks come from here. The allocator's bookkeeping
/// struct lives in KRAM, but the memory it hands out is the tail of URAM, which
/// unprivileged thread code can access.
static USER_HEAP: Heap = Heap::empty();

unsafe extern "C" {
    static _suser_heap: u8;
    static _euser_heap: u8;
    // Bounds of the `.user_text` output section (link.ld).
    static _suser_text: u8;
    static _euser_text: u8;
}

/// True if `addr` (a function address; the Thumb bit is ignored) lies in user
/// flash, i.e. code that unprivileged threads may execute.
fn is_user_text(addr: usize) -> bool {
    let addr = addr & !1;
    let start = addr_of!(_suser_text) as usize;
    let end = addr_of!(_euser_text) as usize;
    addr >= start && addr < end
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Ready,
    Running,
    Blocked,
    Sleeping,
}

// PendSVHandler stores the saved SP at byte offset 4 (`str r2, [r1, #4]`), so
// `priority` then `sp` must stay the first two fields.
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

type ThreadId = usize;

fn os_tick() {
    unsafe { scheduler().tick() };
    os_yield();
}

#[no_mangle]
pub extern "C" fn SystickHandler() {
    //dprintln!("SysTick!");
    os_tick();
}

// Saved exception frame layout (for a fresh stack)
// ^ 0xFFFF_FFFF
// |
// -1   | PSR -- (top of stack)
// -2  | PC   |
// -3  | LR   |
// -4  | R12  | popped by returning from an exception
// -5  | R3   |
// -6  | R2   |
// -7  | R1   |
// -8  | R0 ---
// -9  | R4 ----
// -10  | R5    |
// -11 | R6    |
// -12 | R7    |
// -13 | R8    | popped manually by the PendSV Handler
// -14 | R9    |
// -15 | R10   |
// -16 | R11 ---   <---- current stack pointer
// |
// V 0x0000_0000
pub type ThreadEntry = extern "C" fn() -> !;
fn create_tcb(thread_entry: ThreadEntry, stack_words: usize, priority: u8) {
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
        let base = USER_HEAP.alloc_zeroed(layout);
        assert!(!base.is_null(), "out of user memory for thread stack");

        let stack_top = base.add(stack_words * 4) as *mut u32;
        let sp = stack_top.sub(16);
        let lr = stack_top.sub(3);
        let pc = stack_top.sub(2);
        let psr = stack_top.sub(1);

        // Thread entries are `-> !`, so this LR is never used. Poison it so a
        // bad return faults loudly instead of running garbage.
        *lr = 0xFFFF_FFFF;
        *pc = (thread_entry as usize as u32) | 1; // Thumb bit
        *psr = 1 << 24; // Set thumb bit

        scheduler().add_thread(TCB {
            id: 0,
            priority,
            sp,
            stack_base: base,
            stack_layout: layout,
            state: ThreadState::Ready,
            sleep_ms: 0,
        });
    }
}

#[bitflag(u8)]
enum Nrf52840InterruptPriority {
    #[base]
    Zero = 0x00,
    One = 0x20,
    Two = 0x40,
    Three = 0x60,
    Four = 0x80,
    Five = 0xA0,
    Six = 0xC0,
    Seven = 0xE0,
}

fn start_systick() {
    unsafe {
        let core_periph = core_periph();

        core_periph.SYST.disable_interrupt();
        core_periph.SYST.set_reload(64_000_000 / 1000); // 1000 Hz (1 ms period) tuned for renode
        core_periph.SYST.clear_current();
        core_periph.SCB.set_priority(
            cortex_m::peripheral::scb::SystemHandler::SysTick,
            Nrf52840InterruptPriority::Six.into_bits(),
        );
        // Same priority as SysTick, so a syscall (which touches the scheduler) and
        // the tick can never preempt each other; both stay above PendSV, so a
        // context switch never lands in the middle of either.
        core_periph.SCB.set_priority(
            cortex_m::peripheral::scb::SystemHandler::SVCall,
            Nrf52840InterruptPriority::Six.into_bits(),
        );
        // PendSV must not preempt SysTick before the active exception can return.
        core_periph.SCB.set_priority(
            cortex_m::peripheral::scb::SystemHandler::PendSV,
            Nrf52840InterruptPriority::Seven.into_bits(),
        );
        core_periph.SYST.enable_interrupt();
        core_periph.SYST.enable_counter();
    }
}

fn os_init() {
    unsafe {
        embedded_alloc::init!(HEAP, 16000);

        let start = addr_of!(_suser_heap) as usize;
        let end = addr_of!(_euser_heap) as usize;
        USER_HEAP.init(start, end - start);
    }
    os_add_thread(userlib::idle_thread, 1024, 7);
}

/// Register a thread. Call only from `user_main` (privileged, before the
/// scheduler starts). `priority` 0 is highest, 7 is the idle thread's.
pub fn os_add_thread(thread_entry: ThreadEntry, stack_size: usize, priority: u8) {
    create_tcb(thread_entry, stack_size, priority);
}

/// Called once from privileged boot code. Boot code cannot drop privilege
/// itself: after `msr control` the next instruction would already run
/// unprivileged on the kernel stack. Instead we trap into SVC, where handler
/// mode sets nPRIV and PendSV exception-returns into the first thread.
fn os_start() -> ! {
    // The SVC handler writes a return value into the stacked r0 (see
    // userlib's syscall! macro), so r0 must be declared clobbered here too.
    unsafe { asm!("svc #0", lateout("r0") _) };
    panic!("os_start should never return");
}

/// Pend PendSV: the switch happens once no higher-priority exception is
/// active. A plain volatile store; the previous inline asm wrote r0/r1 without
/// declaring them, which corrupted live registers wherever it was inlined.
pub extern "C" fn context_switch() {
    cortex_m::peripheral::SCB::set_pendsv();
}

/// Kernel-internal yield. Handler mode only (SysTick, SVC, ISRs): PendSV has
/// the lowest priority, so the switch happens on exception return. Threads
/// call `sys_yield`.
fn os_yield() {
    context_switch();
}

#[no_mangle]
extern "C" fn pick_next_thread() {
    unsafe { scheduler().schedule() }
}

pub fn os_get_frc() -> u32 {
    cortex_m::peripheral::SYST::get_current()
}
