//! Cortex-M4 vector table for the nRF52840.
//!
//! Layout (ARMv7-M): initial SP, reset, 14 system exceptions, then external
//! interrupts. The nRF52840 uses IRQ numbers 0..=47 (e.g. UARTE0 is IRQ 2,
//! RTC0 is IRQ 11); see the product specification for the full list.
//!
//! The table is placed in `.vector_table`, which `link.ld` puts at address 0.
use core::arch::global_asm;
use core::panic::PanicInfo;

use cortex_m::Peripherals;

use crate::global::Global;
use crate::reset::ResetHandler;
use crate::uarte::uarte0_handler;
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
        handler(PendSVHandler),   // 14 PendSV
        handler(SystickHandler),  // 15 SysTick
    ],
    irqs: [
        handler(default_handler), // 0 POWER_CLOCK
        handler(default_handler), // 1 RADIO
        handler(uarte0_handler),  // 2 UARTE0_UART0
        handler(default_handler), // 3 SPIM0_SPIS0_TWIM0_TWIS0
        handler(default_handler), // 4 SPIM1_SPIS1_TWIM1_TWIS1
        handler(default_handler), // 5 NFCT
        handler(default_handler), // 6 GPIOTE
        handler(default_handler), // 7 SAADC
        handler(default_handler), // 8 TIMER0
        handler(default_handler), // 9 TIMER1
        handler(default_handler), // 10 TIMER2
        handler(default_handler), // 11 RTC0
        handler(default_handler), // 12 TEMP
        handler(default_handler), // 13 RNG
        handler(default_handler), // 14 ECB
        handler(default_handler), // 15 CCM_AAR
        handler(default_handler), // 16 WDT
        handler(default_handler), // 17 RTC1
        handler(default_handler), // 18 QDEC
        handler(default_handler), // 19 COMP_LPCOMP
        handler(default_handler), // 20 SWI0_EGU0
        handler(default_handler), // 21 SWI1_EGU1
        handler(default_handler), // 22 SWI2_EGU2
        handler(default_handler), // 23 SWI3_EGU3
        handler(default_handler), // 24 SWI4_EGU4
        handler(default_handler), // 25 SWI5_EGU5
        handler(default_handler), // 26 TIMER3
        handler(default_handler), // 27 TIMER4
        handler(default_handler), // 28 PWM0
        handler(default_handler), // 29 PDM
        RESERVED,                 // 30 Reserved
        RESERVED,                 // 31 Reserved
        handler(default_handler), // 32 MWU
        handler(default_handler), // 33 PWM1
        handler(default_handler), // 34 PWM2
        handler(default_handler), // 35 SPIM2_SPIS2
        handler(default_handler), // 36 RTC2
        handler(default_handler), // 37 I2S
        handler(default_handler), // 38 FPU
        handler(default_handler), // 39 USBD
        handler(default_handler), // 40 UARTE1
        handler(default_handler), // 41 QSPI
        handler(default_handler), // 42 CRYPTOCELL
        RESERVED,                 // 43 Reserved
        RESERVED,                 // 44 Reserved
        handler(default_handler), // 45 PWM3
        RESERVED,                 // 46 Reserved
        handler(default_handler), // 47 SPIM3
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
    "2:",
    "ldr r1, =NEXT_TCB",
    "ldr r1, [r1]",
    "str r1, [r0]",
    "ldr r2, [r1, #4]",
    "ldmia r2!, {{r4-r11}}",
    "msr PSP, r2",
    "mvn lr, #2",
    "cpsie i",
    "bx lr",
);

unsafe extern "C" {
    safe fn PendSVHandler();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    dprintln!("Panic: {}", info);
    loop {}
}
