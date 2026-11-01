use crate::uarte::{uart_init, uart_send};
use crate::{os_add_thread, os_init, os_start, os_yield, CURR_TCB, NEXT_TCB, SCHEDULER};
use core::arch::asm;
use core::panic::PanicInfo;

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

#[no_mangle]
pub extern "C" fn PendSVHandler() {
    unsafe {
        asm!(
            "CPSID  I                ",
            "LDR    R0, =CURR_TCB    ",
            "LDR    R1, [R0]         ", // R1 = CURR_TCB
            "CBZ    R1, 1f           ", // first switch: nothing to save
            "MRS    R2, PSP          ", // R2 = outgoing task's stack pointer
            "STMDB  R2!, {R4-R11}    ", // push r4-r11 onto the task stack (PSP), not MSP
            "STR    R2, [R1, #4]     ", // CURR_TCB->sp = R2
            "1:                      ",
            "LDR    R1, =NEXT_TCB    ",
            "LDR    R1, [R1]         ", // R1 = NEXT_TCB
            "STR    R1, [R0]         ", // CURR_TCB = NEXT_TCB
            "LDR    R2, [R1, #4]     ", // R2 = CURR_TCB->sp
            "LDMIA  R2!, {R4-R11}    ", // restore r4-r11 from the task stack
            "MSR    PSP, R2          ", // PSP now points at the hardware frame (r0-r3, r12, lr, pc, xpsr)
            "MVN    LR, #2           ", // LR = 0xFFFFFFFD: return to Thread mode, use PSP
            "CPSIE  I                ",
            "BX     LR               ", // hardware pops the frame from PSP
            options(raw, noreturn)
        )
    }
}

extern "C" fn idle_thread() -> ! {
    loop {
        uart_send(b"Hello, UART!\n");
    }
}

extern "C" fn idle_thread2() -> ! {
    loop {
        uart_send(b"Screw you, UART!\n");
    }
}

#[no_mangle]
pub extern "C" fn SystickHandler() {
    dprintln!("SysTick!");
    os_yield();
}

#[no_mangle]
pub extern "C" fn ResetHandler() -> ! {
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
