//! The architecture ("port") layer: everything the portable kernel needs from
//! the CPU, the boot environment or the linker script lives behind this module.
//!
//!   * `armv7m` - the real thing (Cortex-M4F, bare metal): boot and MPU
//!     (`reset.rs`), vector table and exception handlers (`vectors.rs`), SVC
//!     entry (`svc.rs`), thread frames (`frame.rs`), SysTick, NVIC, heaps, the
//!     debug console and the link.ld symbols (`mod.rs`).
//!   * `host` - a fake used when the crate is built for a host OS, so the
//!     portable kernel can run in host tests. It records what the kernel asked
//!     the "hardware" to do.
//!
//! The portable kernel uses only the functions re-exported below; every
//! backend must provide all of them.
//!
//! | function                       | armv7m                                  | host                                |
//! |--------------------------------|-----------------------------------------|-------------------------------------|
//! | `pend_context_switch`          | set ICSR.PENDSVSET (handler mode only)  | set a pending flag                  |
//! | `start_systick`                | 1 kHz SysTick + exception priorities    | record that it started              |
//! | `frc`                          | SysTick current value                   | 0                                   |
//! | `nvic_set_priority/enable/unpend` | NVIC registers                       | fake NVIC state                     |
//! | `init_heaps`, `alloc_user_stack` | kernel heap (KRAM), user heap (URAM)  | nothing / the global allocator      |
//! | `user_text_range`, `ksem_range`, `flash_range`, `uram_range` | link.ld symbols | fixed ranges (see `host.rs`) |
//! | `log_putc`                     | debug console @ 0x6000_0000             | capped in-memory buffer             |
//! | `init_stack`                   | initial exception frame of a thread     | same (`armv7m/frame.rs`)            |
//! | `halt`                         | spin forever                            | panic, so a test can observe it     |

#[cfg(target_os = "none")]
mod armv7m;
#[cfg(target_os = "none")]
pub(crate) use self::armv7m::{
    alloc_user_stack, flash_range, frc, halt, init_heaps, init_stack, ksem_range, log_putc,
    nvic_enable, nvic_set_priority, nvic_unpend, pend_context_switch, start_systick, uram_range,
    user_text_range,
};

// The thread frame layout is pure data manipulation; the host port uses the
// same code, so it can be tested on the host.
#[cfg(not(target_os = "none"))]
#[path = "armv7m/frame.rs"]
mod armv7m_frame;
#[cfg(not(target_os = "none"))]
pub(crate) mod host;
#[cfg(not(target_os = "none"))]
pub(crate) use self::host::{
    alloc_user_stack, flash_range, frc, halt, init_heaps, init_stack, ksem_range, log_putc,
    nvic_enable, nvic_set_priority, nvic_unpend, pend_context_switch, start_systick, uram_range,
    user_text_range,
};
