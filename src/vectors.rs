//! Cortex-M4 vector table for the nRF52840.
//!
//! Layout (ARMv7-M): initial SP, reset, 14 system exceptions, then external
//! interrupts. The nRF52840 uses IRQ numbers 0..=47 (e.g. UARTE0 is IRQ 2,
//! RTC0 is IRQ 11); see the product specification for the full list.
//!
//! The table is placed in `.vector_table`, which `link.ld` puts at address 0.

use core::ptr::addr_of;

extern "C" {
    /// Top of RAM, defined in `link.ld`.
    static _estack: u32;
}

/// Number of external interrupt lines on the nRF52840.
const NUM_IRQS: usize = 48;

#[derive(Clone, Copy)]
#[repr(C)]
pub union Vector {
    handler: extern "C" fn(),
    reserved: usize,
}

const fn handler(f: extern "C" fn()) -> Vector {
    Vector { handler: f }
}

const RESERVED: Vector = Vector { reserved: 0 };

#[repr(C)]
pub struct VectorTable {
    initial_sp: *const u32,
    reset: extern "C" fn() -> !,
    exceptions: [Vector; 14],
    irqs: [Vector; NUM_IRQS],
}

// The table only holds addresses of code and RAM; it is never mutated.
unsafe impl Sync for VectorTable {}

#[link_section = ".vector_table"]
#[no_mangle]
#[used]
pub static VECTOR_TABLE: VectorTable = VectorTable {
    initial_sp: addr_of!(_estack),
    reset: crate::Reset,
    exceptions: [
        handler(default_handler), // 2  NMI
        handler(default_handler), // 3  HardFault
        handler(default_handler), // 4  MemManage
        handler(default_handler), // 5  BusFault
        handler(default_handler), // 6  UsageFault
        RESERVED,                 // 7
        RESERVED,                 // 8
        RESERVED,                 // 9
        RESERVED,                 // 10
        handler(default_handler), // 11 SVCall
        handler(default_handler), // 12 DebugMonitor
        RESERVED,                 // 13
        handler(default_handler), // 14 PendSV
        handler(default_handler), // 15 SysTick
    ],
    irqs: [handler(default_handler); NUM_IRQS],
};

/// Placeholder for every exception and interrupt until you install real ones.
/// Spinning here makes an unexpected fault easy to spot: pause the simulator
/// and check `sysbus.cpu PC`.
extern "C" fn default_handler() {
    loop {}
}
