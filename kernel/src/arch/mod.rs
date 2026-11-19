//! The architecture ("port") layer: everything the portable kernel needs from
//! the CPU, the boot environment or the linker script lives behind this module.
//!
//!   * `armv7m` - Cortex-M4F (nRF52840): boot and MPU (`reset.rs`), vector
//!     table and exception handlers (`vectors.rs`), SVC entry (`svc.rs`),
//!     SysTick, NVIC, heaps, the debug console and the link.ld symbols
//!     (`mod.rs`).
//!
//! The portable kernel uses only the functions re-exported below; a new
//! backend must provide all of them.
//!
//! | function                       | armv7m                                  |
//! |--------------------------------|-----------------------------------------|
//! | `pend_context_switch`          | set ICSR.PENDSVSET (handler mode only)  |
//! | `start_systick`                | 1 kHz SysTick + exception priorities    |
//! | `frc`                          | SysTick current value                   |
//! | `nvic_set_priority/enable/unpend` | NVIC registers                       |
//! | `init_heaps`, `alloc_user_stack` | kernel heap (KRAM), user heap (URAM)  |
//! | `user_text_range`, `ksem_range`, `flash_range`, `uram_range` | link.ld symbols |
//! | `log_putc`                     | debug console @ 0x6000_0000             |
//! | `halt`                         | spin forever                            |

mod armv7m;

pub(crate) use self::armv7m::{
    alloc_user_stack, flash_range, frc, halt, init_heaps, ksem_range, log_putc, nvic_enable,
    nvic_set_priority, nvic_unpend, pend_context_switch, start_systick, uram_range,
    user_text_range,
};
