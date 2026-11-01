#![no_std]
#![no_main]

mod nrf52840;
mod vectors;
#[macro_use]
mod debugcon;
mod exception_routines;
mod irq;

use crate::exception_routines::os_first_switch;
use core::ptr;

#[repr(C, align(8))] // AAPCS requires the SP to be 8-byte aligned
struct Stack([u32; 128]);

static mut STACK: Stack = Stack([0; 128]);
static mut THE_TCB: TCB = TCB {
    id: 1,
    sp: ptr::null_mut(),
    stack: ptr::null_mut(),
};

#[repr(C)]
pub struct TCB {
    id: u32,
    sp: *mut u32,
    stack: *mut u32,
}

// Globals used to identify the current and next threads for thr context switch logic
#[no_mangle]
pub static mut CURR_TCB: *mut TCB = ptr::null_mut();

#[no_mangle]
pub static mut NEXT_TCB: *mut TCB = ptr::null_mut();

type ThreadEntry = extern "C" fn() -> !;
fn create_tcb(thread_entry: ThreadEntry) {
    static COUNT: u32 = 0;
    let stack_size_dwords: usize = 256;

    // Saved exception frame layout (for a fresh stack)
    // ^ 0xFFFF_FFFF
    // |
    // -1   | PSR -- (top of stack)
    // -2  | PC   |
    // -3  | LR   |
    // -4  | R12  | popped by returning from an exception
    // -5  | R3   |
    // -6  | R2   |
    // -7  | R1   |
    // -8  | R0 ---
    // -9  | R4 ----
    // -10  | R5    |
    // -11 | R6    |
    // -12 | R7    |
    // -13 | R8    | popped manually by the PendSV Handler
    // -14 | R9    |
    // -15 | R10   |
    // -16 | R11 ---   <---- current stack pointer
    // |
    // V 0x0000_0000

    unsafe {
        let stack_top: *mut u32 = STACK.0.as_mut_ptr().add(stack_size_dwords);
        let sp = stack_top.sub(16);
        let lr = stack_top.sub(3);
        let pc = stack_top.sub(2);
        let psr = stack_top.sub(1);

        THE_TCB.sp = sp;
        THE_TCB.stack = STACK.0.as_mut_ptr();
        dprintln!("{} {:p} {:p}", THE_TCB.id, THE_TCB.sp, THE_TCB.stack);

        // Set up the LR, PC, and PSR
        *lr = 0xFFFF_FFFF; // EXC_RETURN for standard frame, default register, thumb logic
        *pc = thread_entry as usize as u32;
        *psr = 1 << 24; // Set thumb bit
    }
}

fn os_init(thread_entry: ThreadEntry) -> ! {
    unsafe {
        create_tcb(thread_entry);
        NEXT_TCB = &raw mut THE_TCB;
        os_first_switch();
    }
}
