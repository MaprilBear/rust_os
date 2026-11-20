//! Floating point in threads. Known issue 18: nothing enables the FPU
//! (CPACR), so the first floating-point instruction a thread executes
//! raises a UsageFault (NOCP), which escalates to HardFault and stops the
//! system. Enabling it also needs the context switch to save s16-s31 and use
//! the FP-aware EXC_RETURN. Its Robot test is tagged `known-issue`.
#![no_std]
#![no_main]

use core::arch::asm;

use ktest::tests;
use userlib::{sys_yield, Semaphore};

static GO_CLOBBER: Semaphore = kernel::semaphore!(0);

#[no_mangle]
pub extern "C" fn user_main() {
    kernel::os_add_thread(clobber, 256, ktest::WORKER_PRIORITY);
    ktest::start(
        "kernel_fpu",
        tests![
            float_math_works_in_a_thread,
            float_registers_survive_context_switches
        ],
    );
}

/// Writes s16 (callee-saved), then yields.
extern "C" fn clobber() -> ! {
    loop {
        GO_CLOBBER.take();
        unsafe { asm!("vmov.f32 s16, #-1.0", out("s16") _) };
        sys_yield();
    }
}

fn float_math_works_in_a_thread() {
    let x = core::hint::black_box(1.5f32);
    assert_eq!(x * 2.0 + 0.25, 3.25);
}

fn float_registers_survive_context_switches() {
    GO_CLOBBER.give();
    let x = core::hint::black_box(1.5f32);
    let s16: f32;
    unsafe {
        asm!(
            "vmov.f32 s16, {x}",
            "svc #1", // Yield: `clobber` writes s16
            "vmov.f32 {out}, s16",
            x = in(sreg) x, out = out(sreg) s16, out("r0") _, out("s16") _,
        )
    };
    assert_eq!(s16, 1.5);
}
