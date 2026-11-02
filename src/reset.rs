use core::{arch::asm, ptr::{addr_of, addr_of_mut}};

use crate::{exception_routines::{idle_thread, idle_thread2}, os_add_thread, os_init, os_start, uarte::uart_init};

unsafe extern "C" {
    static _sidata: u32;
    static mut _sdata: u32;
    static mut _edata: u32;
    static mut _sbss: u32;
    static mut _ebss: u32;
    pub safe fn PendSVHandler();
}

/// Copy `.data` from flash to RAM and zero `.bss`.
///
/// # Safety
/// Must be called once, before any Rust code that reads statics.
/// The linker script must word-align `_sdata`, `_edata`, `_sidata`, `_sbss`, `_ebss`.
#[inline(never)]
unsafe fn initialize_memory() {
    // .data: flash -> RAM, 4 words per iteration.
    let mut src = addr_of!(_sidata);
    let mut dst = addr_of_mut!(_sdata);
    let data_end = addr_of_mut!(_edata);

    while data_end.offset_from(dst) >= 4 {
        asm!(
            "ldm {s}!, {{{t0}, {t1}, {t2}, {t3}}}",
            "stm {d}!, {{{t0}, {t1}, {t2}, {t3}}}",
            s = inout(reg) src,
            d = inout(reg) dst,
            t0 = out(reg) _,
            t1 = out(reg) _,
            t2 = out(reg) _,
            t3 = out(reg) _,
            options(nostack)
        );
    }
    while dst < data_end {
        core::ptr::write_volatile(dst, core::ptr::read_volatile(src));
        dst = dst.add(1);
        src = src.add(1);
    }

    // .bss: zero, 4 words per iteration.
    let mut bss = addr_of_mut!(_sbss);
    let bss_end = addr_of_mut!(_ebss);

    while bss_end.offset_from(bss) >= 4 {
        asm!(
            "movs {t0}, #0",
            "movs {t1}, #0",
            "movs {t2}, #0",
            "movs {t3}, #0",
            "stm {d}!, {{{t0}, {t1}, {t2}, {t3}}}",
            d = inout(reg) bss,
            t0 = out(reg) _,
            t1 = out(reg) _,
            t2 = out(reg) _,
            t3 = out(reg) _,
            options(nostack)
        );
    }
    while bss < bss_end {
        core::ptr::write_volatile(bss, 0);
        bss = bss.add(1);
    }
}

#[no_mangle]
pub extern "C" fn ResetHandler() -> ! {
    unsafe { initialize_memory() };
    dprintln!("entered reset");
    os_init();
    os_add_thread(idle_thread, 256);
    os_add_thread(idle_thread2, 256);
    uart_init();
    os_start();
}