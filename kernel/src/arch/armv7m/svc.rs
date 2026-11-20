//! SVCall entry: decode the caller's stacked frame and hand the call to the
//! portable dispatcher (`crate::syscall::dispatch`).

use core::arch::global_asm;

use crate::syscall::{self, Caller};

// SVCall entry. Picks the stack the caller was using (EXC_RETURN bit 2), then
// branches (not calls) to the Rust dispatcher with r0 = stacked frame and
// r1 = EXC_RETURN. LR is untouched, so the dispatcher's own return performs the
// exception return.
global_asm!(
    ".global SvcHandler",
    ".type SvcHandler, %function",
    ".thumb_func",
    "SvcHandler:",
    "tst lr, #4",
    "ite eq",
    "mrseq r0, msp",
    "mrsne r0, psp",
    "mov r1, lr",
    "b svc_dispatch",
);

#[no_mangle]
extern "C" fn svc_dispatch(frame: *mut u32, exc_return: u32) {
    // Stacked frame: r0 r1 r2 r3 r12 LR PC xPSR. The SVC number is the byte before the stacked PC.
    let pc = unsafe { frame.add(6).read() };
    let arg0 = unsafe { frame.read() } as usize;
    let arg1 = unsafe { frame.add(1).read() } as usize;
    let num = unsafe { ((pc - 2) as *const u8).read() };
    // EXC_RETURN low nibble 0x9: from thread mode on MSP, privileged. Only the
    // boot code runs that way; threads use PSP (0xD).
    let caller = if exc_return & 0xF == 0x9 {
        Caller::Boot
    } else {
        Caller::Thread
    };

    let r0 = syscall::dispatch(num, arg0, arg1, caller);

    unsafe { frame.write(r0 as u32) };
}
