//! Hostile syscalls from an unprivileged thread: the kernel must refuse what
//! a thread may not do, return an error, and carry on.
#![no_std]
#![no_main]

use core::arch::asm;

use ktest::tests;
use userlib::{
    sys_debug_write, sys_expect_fault, sys_sem_give, sys_sem_take, sys_yield, Semaphore,
};

static SEM: Semaphore = kernel::semaphore!(1);
static NOT_A_SEMAPHORE: [u32; 8] = [0; 8];
static mut USER_WORD: u32 = 0;

const KERNEL_RAM: usize = 0x2000_0000;
const DEBUG_CONSOLE: usize = 0x6000_0000;
const PERIPHERALS: usize = 0x4000_0000;
const URAM_END: usize = 0x2004_0000;

#[no_mangle]
pub extern "C" fn user_main() {
    ktest::start(
        "kernel_syscalls",
        tests![
            start_from_a_thread_is_refused,
            unknown_syscall_numbers_return_to_the_caller,
            semaphore_handles_must_be_kernel_objects,
            debug_write_reads_only_what_the_thread_may_read,
            expect_fault_writes_only_user_ram,
            the_thread_carries_on_after_refusals,
        ],
    );
}

/// `svc #N` with r0 = 0, r1 = 0; returns r0.
macro_rules! raw_svc {
    ($n:literal) => {{
        let r0: usize;
        unsafe { asm!(concat!("svc #", $n), inout("r0") 0usize => r0, in("r1") 0usize) };
        r0
    }};
}

/// Start already ran at boot, so this cannot tell the caller check from the
/// run-once check; the host tests cover each.
fn start_from_a_thread_is_refused() {
    assert_eq!(raw_svc!(0), 1);
}

/// What they return is known issue 12 (0, read as success); here: the
/// kernel survives them and returns.
fn unknown_syscall_numbers_return_to_the_caller() {
    raw_svc!(8);
    raw_svc!(100);
    raw_svc!(255);
}

fn semaphore_handles_must_be_kernel_objects() {
    let fake = NOT_A_SEMAPHORE.as_ptr() as usize;
    let real = SEM.handle();
    for bad in [0, 1, fake, real + 1, real + 4, 0x2000_7000, usize::MAX] {
        assert_eq!(sys_sem_take(bad), 1, "take({bad:#x})");
        assert_eq!(sys_sem_give(bad), 1, "give({bad:#x})");
    }
    assert_eq!(sys_sem_take(real), 0);
    assert_eq!(sys_sem_give(real), 0);
}

fn debug_write_reads_only_what_the_thread_may_read() {
    for (ptr, len) in [
        (KERNEL_RAM, 4),
        (0x0000_0100, 16), // kernel flash
        (DEBUG_CONSOLE, 4),
        (PERIPHERALS, 4),
        (URAM_END - 4, 8), // runs off the end of user RAM
        (usize::MAX - 1, 4),
    ] {
        assert_eq!(sys_debug_write(ptr, len), 1, "{ptr:#x}+{len}");
    }
    // From flash (a string literal) and from RAM (the stack).
    let text = "syscall probe: flash\n";
    assert_eq!(sys_debug_write(text.as_ptr() as usize, text.len()), 0);
    let stack = *b"syscall probe: stack\n";
    assert_eq!(sys_debug_write(stack.as_ptr() as usize, stack.len()), 0);
    assert_eq!(sys_debug_write(stack.as_ptr() as usize, 0), 0, "nothing");
    // At most 256 bytes (flash is readable, so only the length is wrong).
    assert_eq!(sys_debug_write(text.as_ptr() as usize, 257), 1);
}

fn expect_fault_writes_only_user_ram() {
    let word = &raw mut USER_WORD as usize;
    let flash = NOT_A_SEMAPHORE.as_ptr() as usize; // a static in flash
    for bad in [KERNEL_RAM, word + 1, flash, PERIPHERALS] {
        assert_eq!(sys_expect_fault(bad, 1), 1, "{bad:#x}");
    }
    assert_eq!(sys_expect_fault(word, 1), 0);
    assert_eq!(sys_expect_fault(0, 0), 0, "disarm");
}

fn the_thread_carries_on_after_refusals() {
    for _ in 0..3 {
        assert_eq!(sys_sem_take(0), 1);
        sys_yield();
    }
}
