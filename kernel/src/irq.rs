//! External interrupts: the IRQ numbers, the table of handlers the application
//! registers, and the NVIC operations drivers need.
//!
//! Portable: the vector table and `irq_dispatch`, which reads this table, live
//! in `arch::armv7m`; NVIC register writes go through `arch`.

use userlib::Global;

use crate::arch;

/// Number of external interrupt lines on the nRF52840.
pub const NUM_IRQS: usize = 48;

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
/// Handlers registered by the application, indexed by IRQ number. Lives in kernel
/// RAM, so only privileged code can change it.
static IRQ_TABLE: Global<[Option<extern "C" fn()>; NUM_IRQS]> = Global::new([None; NUM_IRQS]);

/// The handler registered for IRQ line `irq`, if any. Called by the
/// architecture's common IRQ entry.
pub(crate) fn handler_for(irq: usize) -> Option<extern "C" fn()> {
    unsafe { (*IRQ_TABLE.as_ptr()).get(irq).copied().flatten() }
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
    critical_section::with(|_| unsafe {
        (*IRQ_TABLE.as_ptr())[irq as usize] = Some(handler);
    });
}

/// Set the NVIC priority of `irq` (nRF52840: the top 3 bits count, 0x00 is
/// highest). Privileged code only (`user_main` or an ISR).
pub fn irq_set_priority(irq: Interrupt, priority: u8) {
    arch::nvic_set_priority(irq, priority);
}

/// Enable `irq` in the NVIC. Privileged code only. Panics if no handler is
/// attached (`irq_attach`): the interrupt would end up in the default handler,
/// which spins forever.
pub fn irq_enable(irq: Interrupt) {
    let attached =
        critical_section::with(|_| unsafe { (*IRQ_TABLE.as_ptr())[irq as usize].is_some() });
    assert!(attached, "irq_enable before irq_attach");
    arch::nvic_enable(irq);
}

/// Clear `irq`'s pending bit in the NVIC. Privileged code only.
pub fn irq_unpend(irq: Interrupt) {
    arch::nvic_unpend(irq);
}
