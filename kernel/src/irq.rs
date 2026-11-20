//! External interrupts: the IRQ numbers, the table of handlers the application
//! registers, and the NVIC operations drivers need.
//!
//! Portable: the vector table and `irq_dispatch`, which reads this table, live
//! in `arch::armv7m`; NVIC register writes go through `arch`.

use core::ptr;
use core::sync::atomic::{AtomicPtr, Ordering};

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
/// Handlers registered by the application, indexed by IRQ number (null: none).
/// Lives in kernel RAM, so only privileged code can change it. Atomic, so the
/// IRQ entry can read it without masking interrupts: `irq_attach` publishes
/// with Release, readers load with Acquire.
static IRQ_TABLE: [AtomicPtr<()>; NUM_IRQS] = [const { AtomicPtr::new(ptr::null_mut()) }; NUM_IRQS];

/// The handler registered for IRQ line `irq`, if any. Called by the
/// architecture's common IRQ entry.
#[cfg_attr(not(target_os = "none"), allow(dead_code))] // host: no vector table
pub(crate) fn handler_for(irq: usize) -> Option<extern "C" fn()> {
    let handler = IRQ_TABLE.get(irq)?.load(Ordering::Acquire);
    // Safety: only `irq_attach` stores non-null values, and it stores
    // `extern "C" fn()` pointers.
    (!handler.is_null())
        .then(|| unsafe { core::mem::transmute::<*mut (), extern "C" fn()>(handler) })
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
    IRQ_TABLE[irq as usize].store(handler as *mut (), Ordering::Release);
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
    assert!(
        handler_for(irq as usize).is_some(),
        "irq_enable before irq_attach"
    );
    arch::nvic_enable(irq);
}

/// Clear `irq`'s pending bit in the NVIC. Privileged code only.
pub fn irq_unpend(irq: Interrupt) {
    arch::nvic_unpend(irq);
}

/// Test support: forget every registered handler.
#[cfg(all(not(target_os = "none"), any(test, feature = "test-support")))]
pub(crate) fn reset() {
    for slot in &IRQ_TABLE {
        slot.store(ptr::null_mut(), Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;
    use core::sync::atomic::AtomicUsize;

    static CALLS: AtomicUsize = AtomicUsize::new(0);
    static IN_HANDLER: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn counting_handler() {
        CALLS.fetch_add(1, Ordering::SeqCst);
        if userlib::in_handler_mode() {
            IN_HANDLER.fetch_add(1, Ordering::SeqCst);
        }
    }

    extern "C" fn other_handler() {}

    /// Every line the enum names (the nRF52840 has 43; the rest are reserved).
    const ALL: [Interrupt; 43] = {
        use Interrupt::*;
        [
            POWER_CLOCK,
            RADIO,
            UARTE0_UART0,
            SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0,
            SPIM1_SPIS1_TWIM1_TWIS1_SPI1_TWI1,
            NFCT,
            GPIOTE,
            SAADC,
            TIMER0,
            TIMER1,
            TIMER2,
            RTC0,
            TEMP,
            RNG,
            ECB,
            CCM_AAR,
            WDT,
            RTC1,
            QDEC,
            COMP_LPCOMP,
            SWI0_EGU0,
            SWI1_EGU1,
            SWI2_EGU2,
            SWI3_EGU3,
            SWI4_EGU4,
            SWI5_EGU5,
            TIMER3,
            TIMER4,
            PWM0,
            PDM,
            MWU,
            PWM1,
            PWM2,
            SPIM2_SPIS2_SPI2,
            RTC2,
            I2S,
            FPU,
            USBD,
            UARTE1,
            QSPI,
            CRYPTOCELL,
            PWM3,
            SPIM3,
        ]
    };

    #[test]
    fn interrupt_numbers_fit_the_table_and_match_the_nrf52840() {
        let mut seen = [false; NUM_IRQS];
        for irq in &ALL {
            let n = *irq as usize;
            assert!(n < NUM_IRQS, "{irq:?}");
            assert!(!seen[n], "{irq:?} duplicated");
            seen[n] = true;
        }
        // Product specification, section "Instantiation".
        assert_eq!(Interrupt::UARTE0_UART0 as u16, 2);
        assert_eq!(Interrupt::TIMER0 as u16, 8);
        assert_eq!(Interrupt::RTC0 as u16, 11);
        assert_eq!(Interrupt::SWI0_EGU0 as u16, 20);
        assert_eq!(Interrupt::UARTE1 as u16, 40);
        assert_eq!(Interrupt::SPIM3 as u16, 47);
        // Reserved lines on the nRF52840.
        for n in [30, 31, 43, 44, 46] {
            assert!(!seen[n], "IRQ {n} is reserved");
        }
    }

    #[test]
    fn attached_handler_runs_in_handler_mode() {
        let _k = lock();
        CALLS.store(0, Ordering::SeqCst);
        IN_HANDLER.store(0, Ordering::SeqCst);
        assert!(handler_for(Interrupt::TIMER1 as usize).is_none());
        irq_attach(Interrupt::TIMER1, counting_handler);
        raise_irq(Interrupt::TIMER1);
        raise_irq(Interrupt::TIMER1);
        assert_eq!(CALLS.load(Ordering::SeqCst), 2);
        assert_eq!(IN_HANDLER.load(Ordering::SeqCst), 2);
        assert!(!userlib::in_handler_mode(), "thread mode afterwards");
    }

    #[test]
    fn attach_replaces_the_previous_handler() {
        let _k = lock();
        // One cast, compared with itself (see saved_sp_points_at_the_initial_frame).
        let handler: extern "C" fn() = counting_handler;
        irq_attach(Interrupt::RTC0, other_handler);
        irq_attach(Interrupt::RTC0, handler);
        assert_eq!(
            handler_for(Interrupt::RTC0 as usize).map(|h| h as usize),
            Some(handler as usize)
        );
    }

    #[test]
    fn handlers_are_per_line() {
        let _k = lock();
        irq_attach(Interrupt::RTC0, counting_handler);
        assert!(handler_for(Interrupt::RTC1 as usize).is_none());
        assert!(handler_for(Interrupt::RTC0 as usize).is_some());
    }

    #[test]
    fn lines_past_the_table_have_no_handler() {
        let _k = lock();
        assert!(handler_for(NUM_IRQS).is_none());
        assert!(handler_for(usize::MAX).is_none());
        // irq_dispatch computes (IPSR - 16) with wrapping: an exception below
        // 16 must not alias a real line.
        assert!(handler_for(0usize.wrapping_sub(16)).is_none());
    }

    #[test]
    #[should_panic(expected = "IRQ handler is not in user text")]
    fn handler_outside_user_text_is_rejected() {
        let _k = lock();
        let h = counting_handler as extern "C" fn() as usize;
        set_user_text_range(h + 0x100..h + 0x200);
        irq_attach(Interrupt::RTC0, counting_handler);
    }

    #[test]
    #[should_panic(expected = "irq_enable before irq_attach")]
    fn enable_before_attach_panics() {
        let _k = lock();
        irq_enable(Interrupt::TIMER0);
    }

    #[test]
    fn enable_sets_only_that_line_including_lines_above_31() {
        let _k = lock();
        for irq in [Interrupt::UARTE0_UART0, Interrupt::UARTE1, Interrupt::SPIM3] {
            irq_attach(irq, counting_handler);
            irq_enable(irq);
        }
        for irq in &ALL {
            let expect = matches!(
                irq,
                Interrupt::UARTE0_UART0 | Interrupt::UARTE1 | Interrupt::SPIM3
            );
            assert_eq!(nvic_is_enabled(*irq), expect, "{irq:?}");
        }
    }

    #[test]
    fn set_priority_and_unpend_reach_the_nvic() {
        let _k = lock();
        irq_set_priority(Interrupt::UARTE1, 0xC0);
        assert_eq!(nvic_priority(Interrupt::UARTE1), 0xC0);
        assert_eq!(nvic_priority(Interrupt::UARTE0_UART0), 0);
        irq_unpend(Interrupt::UARTE1);
        irq_unpend(Interrupt::UARTE1);
        assert_eq!(nvic_unpend_count(Interrupt::UARTE1), 2);
        assert_eq!(nvic_unpend_count(Interrupt::UARTE0_UART0), 0);
    }

    #[test]
    fn lock_forgets_attached_handlers() {
        {
            let _k = lock();
            irq_attach(Interrupt::QDEC, counting_handler);
        }
        let _k = lock();
        assert!(handler_for(Interrupt::QDEC as usize).is_none());
    }
}
