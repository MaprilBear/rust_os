//! Driver known issues that show on the target. Its Robot test is tagged
//! `known-issue` (excluded by default).
//!
//! Known issue 3: `semaphore!` inside `Timer::new` gives every timer the same
//! kernel object, so holding one timer's lock blocks every other timer.
#![no_std]
#![no_main]

use drivers::timer::{TIMER0, TIMER1};
use ktest::tests;

#[no_mangle]
pub extern "C" fn user_main() {
    ktest::start(
        "driver_known_issues",
        tests![two_timers_can_be_locked_at_once = 500],
    );
}

fn two_timers_can_be_locked_at_once() {
    TIMER0.regs_semaphore.with(|| {
        TIMER1.regs_semaphore.with(|| {}); // deadlocks while they share one
    });
}
