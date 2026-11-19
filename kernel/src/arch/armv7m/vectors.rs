//! Cortex-M4 vector table for the nRF52840.
//!
//! Layout (ARMv7-M): initial SP, reset, 14 system exceptions, then external
//! interrupts. The nRF52840 uses IRQ numbers 0..=47 (e.g. UARTE0 is IRQ 2,
//! RTC0 is IRQ 11); see the product specification for the full list.
//!
//! The table is placed in `.vector_table`, which `link.ld` puts at address 0.
//!
//! Every external interrupt goes through `irq_dispatch`, which looks up the
//! handler the application registered with `irq_attach`. That keeps driver code
//! (in the `drivers` and `app` crates, user flash) out of the kernel crate.
use core::arch::{asm, global_asm};

use cortex_m::Peripherals;
use userlib::Global;

use super::reset::ResetHandler;
use crate::SystickHandler;
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
        handler(SystickHandler),    // 15 SysTick
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

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u16)]
#[allow(non_camel_case_types)]
#[allow(dead_code)]
#[allow(clippy::upper_case_acronyms)]
pub enum Interrupt {
    #[doc = "0 - POWER_CLOCK"]
    POWER_CLOCK = 0,
    #[doc = "1 - RADIO"]
    RADIO = 1,
    #[doc = "2 - UARTE0_UART0"]
    UARTE0_UART0 = 2,
    #[doc = "3 - SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0"]
    SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0 = 3,
    #[doc = "4 - SPIM1_SPIS1_TWIM1_TWIS1_SPI1_TWI1"]
    SPIM1_SPIS1_TWIM1_TWIS1_SPI1_TWI1 = 4,
    #[doc = "5 - NFCT"]
    NFCT = 5,
    #[doc = "6 - GPIOTE"]
    GPIOTE = 6,
    #[doc = "7 - SAADC"]
    SAADC = 7,
    #[doc = "8 - TIMER0"]
    TIMER0 = 8,
    #[doc = "9 - TIMER1"]
    TIMER1 = 9,
    #[doc = "10 - TIMER2"]
    TIMER2 = 10,
    #[doc = "11 - RTC0"]
    RTC0 = 11,
    #[doc = "12 - TEMP"]
    TEMP = 12,
    #[doc = "13 - RNG"]
    RNG = 13,
    #[doc = "14 - ECB"]
    ECB = 14,
    #[doc = "15 - CCM_AAR"]
    CCM_AAR = 15,
    #[doc = "16 - WDT"]
    WDT = 16,
    #[doc = "17 - RTC1"]
    RTC1 = 17,
    #[doc = "18 - QDEC"]
    QDEC = 18,
    #[doc = "19 - COMP_LPCOMP"]
    COMP_LPCOMP = 19,
    #[doc = "20 - SWI0_EGU0"]
    SWI0_EGU0 = 20,
    #[doc = "21 - SWI1_EGU1"]
    SWI1_EGU1 = 21,
    #[doc = "22 - SWI2_EGU2"]
    SWI2_EGU2 = 22,
    #[doc = "23 - SWI3_EGU3"]
    SWI3_EGU3 = 23,
    #[doc = "24 - SWI4_EGU4"]
    SWI4_EGU4 = 24,
    #[doc = "25 - SWI5_EGU5"]
    SWI5_EGU5 = 25,
    #[doc = "26 - TIMER3"]
    TIMER3 = 26,
    #[doc = "27 - TIMER4"]
    TIMER4 = 27,
    #[doc = "28 - PWM0"]
    PWM0 = 28,
    #[doc = "29 - PDM"]
    PDM = 29,
    #[doc = "32 - MWU"]
    MWU = 32,
    #[doc = "33 - PWM1"]
    PWM1 = 33,
    #[doc = "34 - PWM2"]
    PWM2 = 34,
    #[doc = "35 - SPIM2_SPIS2_SPI2"]
    SPIM2_SPIS2_SPI2 = 35,
    #[doc = "36 - RTC2"]
    RTC2 = 36,
    #[doc = "37 - I2S"]
    I2S = 37,
    #[doc = "38 - FPU"]
    FPU = 38,
    #[doc = "39 - USBD"]
    USBD = 39,
    #[doc = "40 - UARTE1"]
    UARTE1 = 40,
    #[doc = "41 - QSPI"]
    QSPI = 41,
    #[doc = "42 - CRYPTOCELL"]
    CRYPTOCELL = 42,
    #[doc = "45 - PWM3"]
    PWM3 = 45,
    #[doc = "47 - SPIM3"]
    SPIM3 = 47,
}
unsafe impl cortex_m::interrupt::InterruptNumber for Interrupt {
    #[inline(always)]
    fn number(self) -> u16 {
        self as u16
    }
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// External interrupt dispatch
///////////////////////////////////////////////////////////////////////////////////////////////////

/// Handlers registered by the application, indexed by IRQ number. Lives in kernel
/// RAM, so only privileged code can change it.
static IRQ_TABLE: Global<[Option<extern "C" fn()>; NUM_IRQS]> = Global::new([None; NUM_IRQS]);

/// Common entry for every external interrupt. Runs privileged (handler mode).
extern "C" fn irq_dispatch() {
    let ipsr: u32;
    unsafe { asm!("mrs {}, ipsr", out(reg) ipsr, options(nomem, nostack, preserves_flags)) };
    // Exception number 16 is IRQ 0.
    let irq = (ipsr & 0x1FF).wrapping_sub(16) as usize;
    match unsafe { (*IRQ_TABLE.as_ptr()).get(irq).copied().flatten() } {
        Some(h) => h(),
        None => default_handler(),
    }
}

/// Route `irq` to `handler`. Call from `user_main` (privileged, before the
/// scheduler starts), before enabling the interrupt in the NVIC. The handler
/// runs privileged, but its code must live in user flash (the `drivers` or
/// `app` crate).
pub fn irq_attach(irq: Interrupt, handler: extern "C" fn()) {
    assert!(
        crate::is_user_text(handler as usize),
        "IRQ handler is not in user text"
    );
    cortex_m::interrupt::free(|_| unsafe {
        (*IRQ_TABLE.as_ptr())[irq as usize] = Some(handler);
    });
}

// SAFETY (for the NVIC functions below): cortex-m marks priority changes and
// unmasking `unsafe` because they can break critical sections built on BASEPRI
// or on masking individual interrupts. This kernel's critical sections use
// PRIMASK only (`critical_section` / `interrupt::free`), which neither
// operation affects, so they are safe to expose.

/// Set the NVIC priority of `irq` (nRF52840: the top 3 bits count, 0x00 is
/// highest). Privileged code only (`user_main` or an ISR).
pub fn irq_set_priority(irq: Interrupt, priority: u8) {
    // Write IPR directly instead of going through `core_periph()`, which would
    // create a second `&mut Peripherals` if called from an ISR.
    let nvic = cortex_m::peripheral::NVIC::PTR;
    unsafe { (*nvic).ipr[irq as usize].write(priority) };
}

/// Enable `irq` in the NVIC. Privileged code only. Panics if no handler is
/// attached (`irq_attach`): the interrupt would end up in `default_handler`,
/// which spins forever.
pub fn irq_enable(irq: Interrupt) {
    let attached =
        cortex_m::interrupt::free(|_| unsafe { (*IRQ_TABLE.as_ptr())[irq as usize].is_some() });
    assert!(attached, "irq_enable before irq_attach");
    unsafe { cortex_m::peripheral::NVIC::unmask(irq) };
}

/// Clear `irq`'s pending bit in the NVIC. Privileged code only.
pub fn irq_unpend(irq: Interrupt) {
    cortex_m::peripheral::NVIC::unpend(irq);
}

pub static CORE_PERIPHERALS: Global<Option<Peripherals>> = Global::new(None);

pub fn core_periph() -> &'static mut Peripherals {
    // Safety: single core; caller must not hold two of these at once
    // and must not use it from an ISR while thread code holds it.
    unsafe {
        (*CORE_PERIPHERALS.as_ptr())
            .as_mut()
            .expect("CORE not initialized")
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
    #[allow(clippy::empty_loop)]
    loop {}
}

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
    #[allow(clippy::empty_loop)]
    loop {}
}

unsafe extern "C" {
    safe fn PendSVHandler();
    safe fn SvcHandler();
}
