use core::{
    arch::asm,
    ptr::{addr_of, addr_of_mut},
};

use crate::{
    os_add_thread, os_init, os_start,
    threads::{user_thread1, user_thread2},
    uarte::uart_init,
    vectors::CORE_PERIPHERALS,
};

unsafe extern "C" {
    static _sidata: u32;
    static mut _sdata: u32;
    static mut _edata: u32;
    static mut _sbss: u32;
    static mut _ebss: u32;
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
            "ldr {t0}, [{s}]",
            "ldr {t1}, [{s}, #4]",
            "ldr {t2}, [{s}, #8]",
            "ldr {t3}, [{s}, #12]",
            "str {t0}, [{d}]",
            "str {t1}, [{d}, #4]",
            "str {t2}, [{d}, #8]",
            "str {t3}, [{d}, #12]",
            "add.w {s}, {s}, #16",
            "add.w {d}, {d}, #16",
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
            "str {t0}, [{d}]",
            "str {t1}, [{d}, #4]",
            "str {t2}, [{d}, #8]",
            "str {t3}, [{d}, #12]",
            "add.w {d}, {d}, #16",
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
    dprintln!("entered reset");
    unsafe { initialize_memory() };
    unsafe {
        *CORE_PERIPHERALS.as_ptr() = Some(cortex_m::peripheral::Peripherals::take().unwrap())
    };
    os_init();
    os_add_thread(user_thread1, 1024, 0);
    os_add_thread(user_thread2, 1024, 1);
    uart_init();
    os_start();
}
