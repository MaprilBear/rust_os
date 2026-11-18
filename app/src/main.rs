//! Application image: drivers, user threads and their interrupt handlers.
//!
//! Everything in this crate is linked into user flash/RAM (link.ld only claims
//! the `kernel` crate for the privileged regions). Threads run unprivileged and
//! reach the kernel through `userlib` syscalls; driver ISRs run privileged and
//! may call the kernel's ISR API (`kernel::give_from_isr`).
#![allow(future_incompatible)] // saturating_cast vs. a possible future std method
#![no_std]
#![no_main]

mod fifo;
mod mmio;
mod panic;
mod ppi;
mod threads;
mod timer;
mod uarte;

/// Called once by the kernel's reset handler: privileged, on MSP, after the
/// kernel is initialised and before the scheduler starts.
#[no_mangle]
pub extern "C" fn user_main() {
    // Attach the ISR before uart_init enables the NVIC line.
    kernel::irq_attach(kernel::Interrupt::UARTE0_UART0, uarte::uarte0_handler);
    kernel::os_add_thread(threads::user_thread1, 1024, 0);
    kernel::os_add_thread(threads::user_thread2, 1024, 1);
    uarte::uart_init();
}
