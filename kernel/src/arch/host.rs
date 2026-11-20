//! Host backend: a fake port for running the portable kernel in host tests.
//! See `arch/mod.rs` for the interface.
//!
//! Nothing here touches hardware. Each operation records what the kernel asked
//! for, in global state like the real kernel's. With cfg(test) or the
//! `test-support` feature, `crate::test_support` can set the address ranges,
//! register semaphores, inspect the records and reset everything.

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

/// A settable address range (start, end).
struct RangeCell(AtomicUsize, AtomicUsize);

impl RangeCell {
    const fn new(start: usize, end: usize) -> Self {
        Self(AtomicUsize::new(start), AtomicUsize::new(end))
    }
    fn get(&self) -> Range<usize> {
        self.0.load(SeqCst)..self.1.load(SeqCst)
    }
    #[cfg_attr(not(any(test, feature = "test-support")), allow(dead_code))]
    fn set(&self, r: Range<usize>) {
        self.0.store(r.start, SeqCst);
        self.1.store(r.end, SeqCst);
    }
}

/// `.user_text`. Default: everything, so host functions are accepted as thread
/// entries and IRQ handlers.
static USER_TEXT: RangeCell = RangeCell::new(0, usize::MAX);
/// `.ksem`. Default: empty. There is no such section on a host, so no
/// semaphore handle is valid unless a test registers it or sets a range.
static KSEM: RangeCell = RangeCell::new(0, 0);
/// Flash and user RAM. Default: empty, so no user pointer is valid.
static FLASH: RangeCell = RangeCell::new(0, 0);
static URAM: RangeCell = RangeCell::new(0, 0);

pub(crate) fn user_text_range() -> Range<usize> {
    USER_TEXT.get()
}

pub(crate) fn ksem_range() -> Range<usize> {
    KSEM.get()
}

pub(crate) fn flash_range() -> Range<usize> {
    FLASH.get()
}

pub(crate) fn uram_range() -> Range<usize> {
    URAM.get()
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

static SYSTICK_STARTED: AtomicBool = AtomicBool::new(false);
/// Like ICSR.PENDSVSET: one bit, however many times it is set.
static PENDSV_PENDING: AtomicBool = AtomicBool::new(false);
/// Bit n set: IRQ n enabled.
static NVIC_ENABLED: AtomicU64 = AtomicU64::new(0);
static NVIC_PRIORITY: [AtomicU8; NUM_IRQS] = [const { AtomicU8::new(0) }; NUM_IRQS];
static NVIC_UNPENDS: [AtomicUsize; NUM_IRQS] = [const { AtomicUsize::new(0) }; NUM_IRQS];

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
const LOG_CAPACITY: usize = 64 * 1024;

static LOG: Mutex<RefCell<Vec<u8>>> = Mutex::new(RefCell::new(Vec::new()));

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

///////////////////////////////////////////////////////////////////////////////////////////////////
// Test support: setters, inspection, semaphore registry, reset
///////////////////////////////////////////////////////////////////////////////////////////////////

#[cfg(all(not(target_os = "none"), any(test, feature = "test-support")))]
pub(crate) mod testing {
    use super::*;

    /// Semaphore handles host tests have registered (see `semaphore::lookup`).
    static KSEM_REGISTERED: Mutex<RefCell<Vec<usize>>> = Mutex::new(RefCell::new(Vec::new()));

    pub(crate) fn ksem_registered(handle: usize) -> bool {
        critical_section::with(|cs| KSEM_REGISTERED.borrow_ref(cs).contains(&handle))
    }

    pub(crate) fn register_ksem(handle: usize) {
        critical_section::with(|cs| {
            let mut registered = KSEM_REGISTERED.borrow_ref_mut(cs);
            if !registered.contains(&handle) {
                registered.push(handle);
            }
        });
    }

    pub(crate) fn set_user_text_range(r: Range<usize>) {
        USER_TEXT.set(r)
    }
    pub(crate) fn set_ksem_range(r: Range<usize>) {
        KSEM.set(r)
    }
    pub(crate) fn set_flash_range(r: Range<usize>) {
        FLASH.set(r)
    }
    pub(crate) fn set_uram_range(r: Range<usize>) {
        URAM.set(r)
    }

    pub(crate) fn systick_started() -> bool {
        SYSTICK_STARTED.load(SeqCst)
    }
    pub(crate) fn pendsv_pending() -> bool {
        PENDSV_PENDING.load(SeqCst)
    }
    /// PendSV was taken: clear the pending bit, as the hardware does.
    pub(crate) fn clear_pendsv() {
        PENDSV_PENDING.store(false, SeqCst);
    }
    pub(crate) fn nvic_is_enabled(irq: Interrupt) -> bool {
        NVIC_ENABLED.load(SeqCst) & (1 << irq as u32) != 0
    }
    pub(crate) fn nvic_priority(irq: Interrupt) -> u8 {
        NVIC_PRIORITY[irq as usize].load(SeqCst)
    }
    pub(crate) fn nvic_unpend_count(irq: Interrupt) -> usize {
        NVIC_UNPENDS[irq as usize].load(SeqCst)
    }
    pub(crate) fn take_log() -> Vec<u8> {
        critical_section::with(|cs| core::mem::take(&mut *LOG.borrow_ref_mut(cs)))
    }

    /// Everything back to its default.
    pub(crate) fn reset() {
        set_user_text_range(0..usize::MAX);
        set_ksem_range(0..0);
        set_flash_range(0..0);
        set_uram_range(0..0);
        critical_section::with(|cs| KSEM_REGISTERED.borrow_ref_mut(cs).clear());
        SYSTICK_STARTED.store(false, SeqCst);
        PENDSV_PENDING.store(false, SeqCst);
        NVIC_ENABLED.store(0, SeqCst);
        for p in &NVIC_PRIORITY {
            p.store(0, SeqCst);
        }
        for u in &NVIC_UNPENDS {
            u.store(0, SeqCst);
        }
        take_log();
    }
}
