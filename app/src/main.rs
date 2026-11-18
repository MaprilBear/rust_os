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
mod timer;
mod uarte;

use crate::uarte::UARTE0;
use userlib::{sys_sleep, sys_yield};

/// Called once by the kernel's reset handler: privileged, on MSP, after the
/// kernel is initialised and before the scheduler starts.
#[no_mangle]
pub extern "C" fn user_main() {
    // Attach the ISR before uart_init enables the NVIC line.
    kernel::irq_attach(kernel::Interrupt::UARTE0_UART0, uarte::uarte0_handler);
    kernel::os_add_thread(user_thread1, 1024, 0);
    kernel::os_add_thread(user_thread2, 1024, 1);
    uarte::uart_init();
}

pub extern "C" fn user_thread1() -> ! {
    loop {
        //sys_uart_write(b"Hello, UART!\n");
        sys_sleep(1);
    }
}

pub extern "C" fn user_thread2() -> ! {
    loop {
        let mut buffer: [u8; 8] = [0; 8];
        let chars = UARTE0.receive(&mut buffer) as usize;
        if chars != 0 {
            let input = core::str::from_utf8(&buffer[..chars]).unwrap_or("Bad input");
            UARTE0.send(input.as_bytes());
        } else {
            sys_yield();
        }
    }
}
