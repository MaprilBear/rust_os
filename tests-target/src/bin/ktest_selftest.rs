//! The harness's own tests. Several fail on purpose; the Robot suite
//! (renode/tests/ktest_selftest.robot) checks every outcome line, so a
//! harness that reports the wrong thing fails there.
#![no_std]
#![no_main]

use ktest::{expect_fault, expect_fault_on, tests, Access, Rng};
use userlib::{sys_sleep, sys_yield};

#[no_mangle]
pub extern "C" fn user_main() {
    ktest::start(
        "ktest_selftest",
        tests![
            passes,
            assertion_fails,
            newline_in_message_is_escaped,
            hangs = 50,
            runs_after_a_hang,
            kernel_ram_read_faults_as_expected,
            no_fault_when_one_was_expected,
            wrong_kind_of_fault,
            runs_after_failures_and_faults,
            rng_follows_the_seed,
            faults_after_its_timeout = 30,
            hangs_after_a_late_fault = 100,
            panics_after_its_timeout = 30,
            passes_after_a_late_panic,
        ],
    );
}

fn passes() {}

fn assertion_fails() {
    let two = core::hint::black_box(2);
    assert_eq!(two, 3, "deliberate");
}

fn newline_in_message_is_escaped() {
    panic!("first\nsecond");
}

fn hangs() {
    loop {
        sys_yield();
    }
}

fn runs_after_a_hang() {}

fn kernel_ram_read_faults_as_expected() {
    // KRAM (0x2000_0000) is privileged-only: an unprivileged read faults.
    expect_fault_on(Access::Data, || unsafe {
        core::ptr::read_volatile(0x2000_0000 as *const u32);
    });
}

fn no_fault_when_one_was_expected() {
    let x = 5u32;
    expect_fault(|| unsafe {
        core::ptr::read_volatile(&x);
    });
}

/// A data fault where an instruction-fetch fault was expected.
fn wrong_kind_of_fault() {
    expect_fault_on(Access::Instruction, || unsafe {
        core::ptr::read_volatile(0x2000_0000 as *const u32);
    });
}

fn runs_after_failures_and_faults() {}

/// Times out, then takes its expected fault while the next test runs: the
/// fault must not count for that test.
fn faults_after_its_timeout() {
    sys_sleep(60);
    expect_fault(|| unsafe {
        core::ptr::read_volatile(0x2000_0000 as *const u32);
    });
}

fn hangs_after_a_late_fault() {
    loop {
        sys_yield();
    }
}

/// Times out, then panics while the next test runs: the panic must not fail
/// that test.
fn panics_after_its_timeout() {
    sys_sleep(60);
    panic!("late panic");
}

fn passes_after_a_late_panic() {
    sys_sleep(80);
}

fn rng_follows_the_seed() {
    let mut a = Rng::new(ktest::seed());
    let mut b = Rng::new(ktest::seed());
    for _ in 0..100 {
        assert_eq!(a.next_u32(), b.next_u32());
    }
    let mut r = Rng::new(1);
    // xorshift32 from 1: 270369, 67634689, 2647435461.
    assert_eq!(
        [r.next_u32(), r.next_u32(), r.next_u32()],
        [270369, 67634689, 2647435461]
    );
    let mut r = Rng::for_run(0);
    for _ in 0..100 {
        assert!(r.below(7) < 7);
    }
}
