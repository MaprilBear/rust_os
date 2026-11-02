use crate::uarte::{UARTE0, uart_init};
use crate::{os_add_thread, os_init, os_start, os_yield};
use core::arch::{asm, global_asm};
use core::panic::PanicInfo;
use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};

// This handler returns via EXC_RETURN, so it must not have a Rust prologue.
global_asm!(
    ".global PendSVHandler",
    ".type PendSVHandler, %function",
    ".thumb_func",
    "PendSVHandler:",
    "cpsid i",
    "ldr r0, =CURR_TCB",
    "ldr r1, [r0]",
    "cbz r1, 1f",
    "mrs r2, PSP",
    "stmdb r2!, {{r4-r11}}",
    "str r2, [r1, #4]",
    "b 2f",
    "1:",
    // The first exception frame was stacked on MSP while starting from ResetHandler.
    "add sp, sp, #32",
    "2:",
    "ldr r1, =NEXT_TCB",
    "ldr r1, [r1]",
    "str r1, [r0]",
    "ldr r2, [r1, #4]",
    "ldmia r2!, {{r4-r11}}",
    "msr PSP, r2",
    "mvn lr, #2",
    "cpsie i",
    "bx lr",
);

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

pub extern "C" fn os_first_switch() -> ! {
    unsafe {
        asm!(
            "CPSIE I",
            "LDR     R0, ={int_ctrl}    ",
            "LDR     R1, ={pendsv}      ", // Execution will never return
            "STR     R1, [R0]           ",
            options(noreturn),
            int_ctrl = const NVIC_INT_CTRL,
            pendsv = const NVIC_PENDSVSET,
        )
    }
}

const NVIC_INT_CTRL: u32 = 0xE000ED04;
const NVIC_PENDSVSET: u32 = 0x10000000;

pub extern "C" fn context_switch() {
    unsafe {
        asm!(
            "LDR     R0, ={int_ctrl}    ",
            "LDR     R1, ={pendsv}      ", // Execution will leave this thread until scheduled again
            "STR     R1, [R0]",
            int_ctrl = const NVIC_INT_CTRL,
            pendsv = const NVIC_PENDSVSET,
        )
    }
}

extern "C" fn idle_thread() -> ! {
    loop {
        unsafe { UARTE0.send(b"Hello, UART!\n") };
    }
}

extern "C" fn idle_thread2() -> ! {
    loop {
        unsafe { UARTE0.send(b"Screw you, UART!\n") };
    }
}

#[no_mangle]
pub extern "C" fn SystickHandler() {
    dprintln!("SysTick!");
    os_yield();
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

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    dprintln!("Panic!");
    loop {}
}

/// Placeholder for every exception and interrupt until you install real ones.
/// Spinning here makes an unexpected fault easy to spot: pause the simulator
/// and check `sysbus.cpu PC`.
pub extern "C" fn default_handler() {
    dprintln!("Exception!");
    #[allow(clippy::empty_loop)]
    loop {}
}
