//! Host backend: a fake port for running the portable kernel in host tests.
//! See `arch/mod.rs` for the interface.
//!
//! Nothing here touches hardware. Each operation records what the kernel asked
//! for, in global state like the real kernel's.

use core::alloc::Layout;
use core::cell::RefCell;
use core::ops::Range;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering::SeqCst};

use alloc::vec::Vec;
use critical_section::Mutex;

use crate::irq::{Interrupt, NUM_IRQS};

pub(crate) use super::armv7m_frame::init_stack;

///////////////////////////////////////////////////////////////////////////////////////////////////
// Address ranges (link.ld symbols on hardware)
///////////////////////////////////////////////////////////////////////////////////////////////////

/// `.user_text`: everything, so host functions are accepted as thread entries
/// and IRQ handlers.
pub(crate) fn user_text_range() -> Range<usize> {
    0..usize::MAX
}

/// `.ksem`: empty. There is no such section on a host, so no semaphore handle
/// is valid unless something registers it.
pub(crate) fn ksem_range() -> Range<usize> {
    0..0
}

/// Flash and user RAM: empty, so no user pointer is valid.
pub(crate) fn flash_range() -> Range<usize> {
    0..0
}

pub(crate) fn uram_range() -> Range<usize> {
    0..0
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Heaps
///////////////////////////////////////////////////////////////////////////////////////////////////

/// The host's global allocator already works; nothing to set up.
pub(crate) fn init_heaps() {}

/// # Safety
/// `layout` must have a non-zero size.
pub(crate) unsafe fn alloc_user_stack(layout: Layout) -> *mut u8 {
    alloc::alloc::alloc_zeroed(layout)
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Fake SysTick / NVIC / PendSV
///////////////////////////////////////////////////////////////////////////////////////////////////

pub(crate) static SYSTICK_STARTED: AtomicBool = AtomicBool::new(false);
/// Like ICSR.PENDSVSET: one bit, however many times it is set.
pub(crate) static PENDSV_PENDING: AtomicBool = AtomicBool::new(false);
/// Bit n set: IRQ n enabled.
pub(crate) static NVIC_ENABLED: AtomicU64 = AtomicU64::new(0);
pub(crate) static NVIC_PRIORITY: [AtomicU8; NUM_IRQS] = [const { AtomicU8::new(0) }; NUM_IRQS];
pub(crate) static NVIC_UNPENDS: [AtomicUsize; NUM_IRQS] = [const { AtomicUsize::new(0) }; NUM_IRQS];

pub(crate) fn start_systick() {
    SYSTICK_STARTED.store(true, SeqCst);
}

pub(crate) fn frc() -> u32 {
    0
}

pub(crate) fn nvic_set_priority(irq: Interrupt, priority: u8) {
    NVIC_PRIORITY[irq as usize].store(priority, SeqCst);
}

pub(crate) fn nvic_enable(irq: Interrupt) {
    NVIC_ENABLED.fetch_or(1 << irq as u32, SeqCst);
}

pub(crate) fn nvic_unpend(irq: Interrupt) {
    NVIC_UNPENDS[irq as usize].fetch_add(1, SeqCst);
}

pub(crate) fn pend_context_switch() {
    PENDSV_PENDING.store(true, SeqCst);
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Log, halt
///////////////////////////////////////////////////////////////////////////////////////////////////

/// Upper bound on buffered log output, so long-running host builds (fuzzers)
/// cannot grow memory without limit. Later bytes are dropped.
pub(crate) const LOG_CAPACITY: usize = 64 * 1024;

pub(crate) static LOG: Mutex<RefCell<Vec<u8>>> = Mutex::new(RefCell::new(Vec::new()));

pub(crate) fn log_putc(byte: u8) {
    critical_section::with(|cs| {
        let mut log = LOG.borrow_ref_mut(cs);
        if log.len() < LOG_CAPACITY {
            log.push(byte);
        }
    });
}

/// On hardware the kernel spins forever; on the host that would hang the test,
/// so panic instead (`#[should_panic]` can then check for it).
pub(crate) fn halt(why: &str) -> ! {
    panic!("kernel halted: {why}")
}
