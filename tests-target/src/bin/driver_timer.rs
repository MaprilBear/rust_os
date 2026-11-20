//! TIMER on the (simulated) peripheral: the register map drives the real
//! hardware block, and a driver's `regs_semaphore` serialises threads that
//! share the peripheral (a smoke test of the lock in a driver static; the
//! semaphore itself is the kernel suites' subject). PPI: `driver_ppi`.
#![no_std]
#![no_main]

use drivers::timer::{TimerBitmode, TimerMode, TIMER1, TIMER2};
use ktest::tests;
use userlib::{sys_yield, Semaphore};

static GO_BUMP: [Semaphore; 2] = [kernel::semaphore!(0), kernel::semaphore!(0)];
static DONE_BUMP: Semaphore = kernel::semaphore!(0);

#[no_mangle]
pub extern "C" fn user_main() {
    kernel::os_add_thread(bumper0, 256, ktest::WORKER_PRIORITY);
    kernel::os_add_thread(bumper1, 256, ktest::WORKER_PRIORITY);
    ktest::start(
        "driver_timer",
        tests![
            timer_counts_in_counter_mode,
            regs_semaphore_serialises_threads_on_a_peripheral,
        ],
    );
}

/// TIMER1 as a 32-bit counter, cleared and started.
fn start_counter() {
    let t = &*TIMER1.regs;
    t.stop.trigger();
    t.mode.write(TimerMode::COUNTER);
    t.bitmode.write(TimerBitmode::BIT_32);
    t.clear.trigger();
    t.start.trigger();
}

fn captured_count() -> u32 {
    let t = &*TIMER1.regs;
    t.capture0.trigger();
    t.cc0.read()
}

fn timer_counts_in_counter_mode() {
    let mut count = 0;
    TIMER1.regs_semaphore.with(|| {
        start_counter();
        for _ in 0..5 {
            TIMER1.regs.count.trigger();
        }
        count = captured_count();
        TIMER1.regs.stop.trigger();
    });
    assert_eq!(count, 5);
}

/// Increments TIMER2's CC[0] register 50 times inside the driver's lock,
/// yielding between the read and the write.
fn bumper(i: usize) -> ! {
    loop {
        GO_BUMP[i].take();
        for _ in 0..50 {
            TIMER2.regs_semaphore.with(|| {
                let v = TIMER2.regs.cc0.read();
                sys_yield();
                TIMER2.regs.cc0.write(v + 1);
            });
        }
        DONE_BUMP.give();
    }
}
extern "C" fn bumper0() -> ! {
    bumper(0)
}
extern "C" fn bumper1() -> ! {
    bumper(1)
}

fn regs_semaphore_serialises_threads_on_a_peripheral() {
    TIMER2.regs_semaphore.with(|| TIMER2.regs.cc0.write(0));
    for go in &GO_BUMP {
        go.give();
    }
    for _ in &GO_BUMP {
        DONE_BUMP.take();
    }
    assert_eq!(TIMER2.regs.cc0.read(), 100);
}
