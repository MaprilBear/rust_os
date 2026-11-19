//! Cortex-M4F (nRF52840) backend. See `arch/mod.rs`.

mod reset;
mod svc;
mod vectors;

use cortex_m::interrupt::InterruptNumber;
use cortex_m::peripheral::NVIC;

use crate::irq::Interrupt;

pub(crate) use self::vectors::core_periph;

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
