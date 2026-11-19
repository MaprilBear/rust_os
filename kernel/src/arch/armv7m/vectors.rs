//! Cortex-M4 vector table for the nRF52840.
//!
//! Layout (ARMv7-M): initial SP, reset, 14 system exceptions, then external
//! interrupts. The nRF52840 uses IRQ numbers 0..=47 (e.g. UARTE0 is IRQ 2,
//! RTC0 is IRQ 11); see the product specification for the full list.
//!
//! The table is placed in `.vector_table`, which `link.ld` puts at address 0.
//!
//! Every external interrupt goes through `irq_dispatch`, which looks up the
//! handler the application registered with `irq_attach` (`crate::irq`). That keeps driver code
//! (in the `drivers` and `app` crates, user flash) out of the kernel crate.
use core::arch::{asm, global_asm};

use super::halt;
use super::reset::ResetHandler;
use crate::irq::{self, NUM_IRQS};
extern "C" {
    /// Top of RAM, defined in `link.ld`.
    static _estack: u32;
}

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
    #[allow(unused_unsafe)]
    initial_sp: unsafe { &raw const _estack },
    reset: ResetHandler,
    exceptions: [
        handler(default_handler),   // 2  NMI
        handler(default_handler),   // 3  HardFault
        handler(memmanage_handler), // 4  MemManage
        handler(default_handler),   // 5  BusFault
        handler(default_handler),   // 6  UsageFault
        RESERVED,                   // 7
        RESERVED,                   // 8
        RESERVED,                   // 9
        RESERVED,                   // 10
        handler(SvcHandler),        // 11 SVCall
        handler(default_handler),   // 12 DebugMonitor
        RESERVED,                   // 13
        handler(PendSVHandler),     // 14 PendSV
        handler(systick_handler),   // 15 SysTick
    ],
    irqs: [
        handler(irq_dispatch), // 0 POWER_CLOCK
        handler(irq_dispatch), // 1 RADIO
        handler(irq_dispatch), // 2 UARTE0_UART0
        handler(irq_dispatch), // 3 SPIM0_SPIS0_TWIM0_TWIS0
        handler(irq_dispatch), // 4 SPIM1_SPIS1_TWIM1_TWIS1
        handler(irq_dispatch), // 5 NFCT
        handler(irq_dispatch), // 6 GPIOTE
        handler(irq_dispatch), // 7 SAADC
        handler(irq_dispatch), // 8 TIMER0
        handler(irq_dispatch), // 9 TIMER1
        handler(irq_dispatch), // 10 TIMER2
        handler(irq_dispatch), // 11 RTC0
        handler(irq_dispatch), // 12 TEMP
        handler(irq_dispatch), // 13 RNG
        handler(irq_dispatch), // 14 ECB
        handler(irq_dispatch), // 15 CCM_AAR
        handler(irq_dispatch), // 16 WDT
        handler(irq_dispatch), // 17 RTC1
        handler(irq_dispatch), // 18 QDEC
        handler(irq_dispatch), // 19 COMP_LPCOMP
        handler(irq_dispatch), // 20 SWI0_EGU0
        handler(irq_dispatch), // 21 SWI1_EGU1
        handler(irq_dispatch), // 22 SWI2_EGU2
        handler(irq_dispatch), // 23 SWI3_EGU3
        handler(irq_dispatch), // 24 SWI4_EGU4
        handler(irq_dispatch), // 25 SWI5_EGU5
        handler(irq_dispatch), // 26 TIMER3
        handler(irq_dispatch), // 27 TIMER4
        handler(irq_dispatch), // 28 PWM0
        handler(irq_dispatch), // 29 PDM
        RESERVED,              // 30 Reserved
        RESERVED,              // 31 Reserved
        handler(irq_dispatch), // 32 MWU
        handler(irq_dispatch), // 33 PWM1
        handler(irq_dispatch), // 34 PWM2
        handler(irq_dispatch), // 35 SPIM2_SPIS2
        handler(irq_dispatch), // 36 RTC2
        handler(irq_dispatch), // 37 I2S
        handler(irq_dispatch), // 38 FPU
        handler(irq_dispatch), // 39 USBD
        handler(irq_dispatch), // 40 UARTE1
        handler(irq_dispatch), // 41 QSPI
        handler(irq_dispatch), // 42 CRYPTOCELL
        RESERVED,              // 43 Reserved
        RESERVED,              // 44 Reserved
        handler(irq_dispatch), // 45 PWM3
        RESERVED,              // 46 Reserved
        handler(irq_dispatch), // 47 SPIM3
    ],
};

///////////////////////////////////////////////////////////////////////////////////////////////////
// External interrupt dispatch
///////////////////////////////////////////////////////////////////////////////////////////////////

/// Common entry for every external interrupt. Runs privileged (handler mode).
extern "C" fn irq_dispatch() {
    let ipsr: u32;
    unsafe { asm!("mrs {}, ipsr", out(reg) ipsr, options(nomem, nostack, preserves_flags)) };
    // Exception number 16 is IRQ 0.
    let irq = (ipsr & 0x1FF).wrapping_sub(16) as usize;
    match irq::handler_for(irq) {
        Some(h) => h(),
        None => default_handler(),
    }
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Handlers
///////////////////////////////////////////////////////////////////////////////////////////////////

/// Placeholder for every exception and interrupt until you install real ones.
/// Spinning here makes an unexpected fault easy to spot: pause the simulator
/// and check `sysbus.cpu PC`.
pub extern "C" fn default_handler() {
    dprintln!("Exception!");
    halt("unexpected exception");
}

/// One scheduler tick (1 ms).
extern "C" fn systick_handler() {
    crate::os_tick();
}

// PendSV saves/loads a thread's SP at byte offset 4 of its TCB.
const _: () = assert!(core::mem::offset_of!(crate::TCB, sp) == 4);

// This handler returns via EXC_RETURN, so it must not have a Rust prologue.
global_asm!(
    ".global PendSVHandler",
    ".type PendSVHandler, %function",
    ".thumb_func",
    "PendSVHandler:",
    "cpsid i",
    "ldr r0, =CURR_TCB",
    "ldr r1, [r0]",
    "cbz r1, 1f",
    "mrs r2, PSP",
    "stmdb r2!, {{r4-r11}}",
    "str r2, [r1, #4]",
    "b 2f",
    "1:",
    // The first exception frame was stacked on MSP while starting from ResetHandler.
    "add sp, sp, #32",
    // First switch only: drop thread mode to unprivileged. Safe to write CONTROL
    // here (handler mode); r2 is reloaded below.
    "mrs r2, control",
    "orr r2, r2, #1",
    "msr control, r2",
    "isb",
    "2:",
    "bl pick_next_thread", // clobbers r0-r3, r12, lr
    "ldr r0, =CURR_TCB",
    "ldr r1, =NEXT_TCB",
    "ldr r1, [r1]",
    "str r1, [r0]",
    "ldr r1, =NEXT_TCB",
    "ldr r1, [r1]",
    "str r1, [r0]",
    "ldr r2, [r1, #4]",
    "ldmia r2!, {{r4-r11}}",
    "msr PSP, r2",
    "mvn lr, #2", // 0xFFFF_FFFD -> EXC_RETURN to thread mode, use PSP
    "cpsie i",
    "bx lr",
);

pub extern "C" fn memmanage_handler() {
    let cfsr = unsafe { core::ptr::read_volatile(0xE000_ED28 as *const u32) };
    let mmfar = unsafe { core::ptr::read_volatile(0xE000_ED34 as *const u32) };
    dprintln!("MemManage: CFSR={:#x} MMFAR={:#x}", cfsr, mmfar);
    halt("MemManage fault");
}

unsafe extern "C" {
    safe fn PendSVHandler();
    safe fn SvcHandler();
}
