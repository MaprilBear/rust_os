//! The architecture ("port") layer: everything the portable kernel needs from
//! the CPU, the boot environment or the linker script lives behind this module.
//!
//!   * `armv7m` - Cortex-M4F (nRF52840): boot, vector table, exception
//!     handlers.

mod armv7m;

pub(crate) use self::armv7m::core_periph;
pub use self::armv7m::{irq_attach, irq_enable, irq_set_priority, irq_unpend, Interrupt};
