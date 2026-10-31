#![no_std]
#![no_main]

mod nrf52840;
mod vectors;
#[macro_use]
mod debugcon;

use core::{panic::PanicInfo, ptr::write_volatile};

use crate::nrf52840::*;

// TODO: Write a better peripheral abstraction layer for the nRF52840. 

// Placeholder reset handler. Replace with your real bring-up code
// (.data copy, .bss zero, FPU enable, jump to kernel main, ...).
#[no_mangle]
pub extern "C" fn Reset() -> ! {
    dprintln!("kernel booted");
    let mut i = 1;
    loop {
        let value = if i % 2 == 0 { 0x0 } else { 0x1 };
        unsafe { write_volatile(&mut gpio0().out as *mut u32, value) };
        i += 1;
        dprintln!("i: {i}")
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
