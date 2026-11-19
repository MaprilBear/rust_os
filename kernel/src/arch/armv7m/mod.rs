//! Cortex-M4F (nRF52840) backend. See `arch/mod.rs`.

mod reset;
mod vectors;

pub(crate) use self::vectors::core_periph;
pub use self::vectors::{irq_attach, irq_enable, irq_set_priority, irq_unpend, Interrupt};
