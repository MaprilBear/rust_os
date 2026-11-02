use crate::uarte::UARTE0;
use crate::os_yield;
use core::arch::{asm, global_asm};
use core::panic::PanicInfo;

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
    pub safe fn PendSVHandler();
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

pub extern "C" fn idle_thread() -> ! {
    loop {
        UARTE0.send(b"Hello, UART!\n");
    }
}

pub extern "C" fn idle_thread2() -> ! {
    loop {
        UARTE0.send(b"Screw you, UART!\n");
    }
}

#[no_mangle]
pub extern "C" fn SystickHandler() {
    dprintln!("SysTick!");
    os_yield();
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
