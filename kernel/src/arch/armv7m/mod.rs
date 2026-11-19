//! Cortex-M4F (nRF52840) backend. See `arch/mod.rs` for the interface.

mod reset;
mod svc;
mod vectors;

use core::alloc::{GlobalAlloc, Layout};
use core::ops::Range;
use core::ptr::addr_of;

use bitfields::bitflag;
use cortex_m::interrupt::InterruptNumber;
use cortex_m::peripheral::scb::SystemHandler;
use cortex_m::peripheral::NVIC;
use cortex_m::Peripherals;
use embedded_alloc::TlsfHeap as Heap;
use userlib::Global;

use crate::irq::Interrupt;

///////////////////////////////////////////////////////////////////////////////////////////////////
// Linker-script symbols (link.ld)
///////////////////////////////////////////////////////////////////////////////////////////////////

unsafe extern "C" {
    static _suser_heap: u8;
    static _euser_heap: u8;
    // Bounds of the `.user_text` output section.
    static _suser_text: u8;
    static _euser_text: u8;
    // Bounds of the `.ksem` input sections inside `.data`.
    static _sksem: u8;
    static _eksem: u8;
    // Ranges the kernel uses to validate user pointers passed to syscalls.
    static _flash_base: u8;
    static _flash_size: u8;
    static _uram_base: u8;
    static _uram_size: u8;
}

/// Code that unprivileged threads may execute (`.user_text`).
pub(crate) fn user_text_range() -> Range<usize> {
    addr_of!(_suser_text) as usize..addr_of!(_euser_text) as usize
}

/// Where `semaphore!` objects live (`.ksem`).
pub(crate) fn ksem_range() -> Range<usize> {
    addr_of!(_sksem) as usize..addr_of!(_eksem) as usize
}

/// All flash.
pub(crate) fn flash_range() -> Range<usize> {
    let base = addr_of!(_flash_base) as usize;
    base..base + addr_of!(_flash_size) as usize
}

/// User RAM.
pub(crate) fn uram_range() -> Range<usize> {
    let base = addr_of!(_uram_base) as usize;
    base..base + addr_of!(_uram_size) as usize
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Heaps
///////////////////////////////////////////////////////////////////////////////////////////////////

/// Kernel heap. Backed by a static in `.bss`, so it lives in KRAM (privileged only).
#[global_allocator]
static HEAP: Heap = Heap::empty();

/// User heap. Only thread stacks come from here. The allocator's bookkeeping
/// struct lives in KRAM, but the memory it hands out is the tail of URAM, which
/// unprivileged thread code can access.
static USER_HEAP: Heap = Heap::empty();

/// Called once by `os_init`, before anything allocates.
pub(crate) fn init_heaps() {
    unsafe {
        embedded_alloc::init!(HEAP, 16000);

        let start = addr_of!(_suser_heap) as usize;
        let end = addr_of!(_euser_heap) as usize;
        USER_HEAP.init(start, end - start);
    }
}

/// Zeroed memory for a thread stack, accessible to unprivileged code. Null if
/// the user heap is exhausted.
///
/// # Safety
/// `layout` must have a non-zero size.
pub(crate) unsafe fn alloc_user_stack(layout: Layout) -> *mut u8 {
    USER_HEAP.alloc_zeroed(layout)
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Core peripherals: SysTick, exception priorities, NVIC
///////////////////////////////////////////////////////////////////////////////////////////////////

static CORE_PERIPHERALS: Global<Option<Peripherals>> = Global::new(None);

/// Called once by the reset handler.
fn take_core_peripherals() {
    unsafe { *CORE_PERIPHERALS.as_ptr() = Some(Peripherals::take().unwrap()) };
}

fn core_periph() -> &'static mut Peripherals {
    // Safety: single core; caller must not hold two of these at once
    // and must not use it from an ISR while thread code holds it.
    unsafe {
        (*CORE_PERIPHERALS.as_ptr())
            .as_mut()
            .expect("CORE not initialized")
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

/// Start the 1 kHz scheduler tick, and set the SysTick, SVCall and PendSV
/// exception priorities the scheduler relies on.
pub(crate) fn start_systick() {
    unsafe {
        let core_periph = core_periph();

        core_periph.SYST.disable_interrupt();
        core_periph.SYST.set_reload(64_000_000 / 1000); // 1000 Hz (1 ms period) tuned for renode
        core_periph.SYST.clear_current();
        core_periph.SCB.set_priority(
            SystemHandler::SysTick,
            Nrf52840InterruptPriority::Six.into_bits(),
        );
        // Same priority as SysTick, so a syscall (which touches the scheduler) and
        // the tick can never preempt each other; both stay above PendSV, so a
        // context switch never lands in the middle of either.
        core_periph.SCB.set_priority(
            SystemHandler::SVCall,
            Nrf52840InterruptPriority::Six.into_bits(),
        );
        // PendSV must not preempt SysTick before the active exception can return.
        core_periph.SCB.set_priority(
            SystemHandler::PendSV,
            Nrf52840InterruptPriority::Seven.into_bits(),
        );
        core_periph.SYST.enable_interrupt();
        core_periph.SYST.enable_counter();
    }
}

/// Free-running counter (SysTick current value).
pub(crate) fn frc() -> u32 {
    core_periph().SYST.cvr.read()
}

unsafe impl InterruptNumber for Interrupt {
    #[inline(always)]
    fn number(self) -> u16 {
        self as u16
    }
}

// SAFETY (for the NVIC functions below): cortex-m marks priority changes and
// unmasking `unsafe` because they can break critical sections built on BASEPRI
// or on masking individual interrupts. This kernel's critical sections use
// PRIMASK only (`critical_section`), which neither operation affects.

pub(crate) fn nvic_set_priority(irq: Interrupt, priority: u8) {
    // Write IPR directly instead of going through `core_periph()`, which would
    // create a second `&mut Peripherals` if called from an ISR.
    unsafe { (*NVIC::PTR).ipr[usize::from(irq.number())].write(priority) };
}

pub(crate) fn nvic_enable(irq: Interrupt) {
    unsafe { NVIC::unmask(irq) };
}

pub(crate) fn nvic_unpend(irq: Interrupt) {
    NVIC::unpend(irq);
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Scheduling, console, halt
///////////////////////////////////////////////////////////////////////////////////////////////////

/// Pend PendSV. Handler mode only (SysTick, SVC, ISRs): PendSV has the lowest
/// priority, so the switch happens when the active exception returns, and no
/// barrier is needed.
pub(crate) fn pend_context_switch() {
    const ICSR: *mut u32 = 0xE000_ED04 as *mut u32;
    const PENDSVSET: u32 = 1 << 28;
    unsafe { ICSR.write_volatile(PENDSVSET) };
}

/// Send one byte to the debug console (`crate::debugcon`).
pub(crate) fn log_putc(byte: u8) {
    /// Must match the address in `renode/nrf52840.repl`.
    const DEBUGCON: *mut u32 = 0x6000_0000 as *mut u32;
    // 32-bit write, because the Renode peripheral is a single 4-byte register.
    unsafe { DEBUGCON.write_volatile(byte as u32) }
}

/// Stop for good. Callers log why first; `_why` names the reason for readers
/// (and lets a host port report it).
pub(crate) fn halt(_why: &str) -> ! {
    #[allow(clippy::empty_loop)]
    loop {}
}
