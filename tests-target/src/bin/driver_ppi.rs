//! PPI on the peripheral: an event routed to a task without the CPU.
//!
//! In Renode the routed task write happens at the next synchronisation
//! point, not at once, so the test lets virtual time pass (a 1 ms sleep)
//! after each event. On hardware the task follows within a few cycles.
#![no_std]
#![no_main]

use drivers::ppi::{ChannelRegBuilder, PPI};
use drivers::timer::{TimerBitmode, TimerMode, TIMER1};
use ktest::tests;
use userlib::sys_sleep;

// EGU1: an event a thread can raise by writing a task, with no interrupt.
const EGU1: usize = 0x4001_5000;
const EGU1_TASKS_TRIGGER0: *mut u32 = EGU1 as *mut u32;
const EGU1_EVENTS_TRIGGERED0: usize = EGU1 + 0x100;

#[no_mangle]
pub extern "C" fn user_main() {
    ktest::start("driver_ppi", tests![ppi_routes_an_event_to_a_task]);
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

/// EGU1's TRIGGERED[0] event -> PPI channel 0 -> TIMER1's COUNT task.
fn ppi_routes_an_event_to_a_task() {
    // Read inside the locks, assert outside: a failing test is parked, and a
    // lock it held would block every later test.
    let (mut routed, mut after_disable) = (0, 0);
    TIMER1.regs_semaphore.with(|| {
        PPI.regs_semaphore.with(|| {
            start_counter();
            let ppi = &*PPI.regs;
            ppi.ch0_eep.write(EGU1_EVENTS_TRIGGERED0 as u32);
            ppi.ch0_tep.write(&TIMER1.regs.count as *const _ as u32);
            ppi.chenset
                .write(ChannelRegBuilder::new().with_ch0(true).build());
            for _ in 0..3 {
                unsafe { EGU1_TASKS_TRIGGER0.write_volatile(1) };
                sys_sleep(1);
            }
            routed = captured_count();
            ppi.chenclear
                .write(ChannelRegBuilder::new().with_ch0(true).build());
            unsafe { EGU1_TASKS_TRIGGER0.write_volatile(1) }; // channel off: no count
            sys_sleep(1); // as long as a routed event would take
            after_disable = captured_count();
            TIMER1.regs.stop.trigger();
        })
    });
    assert_eq!(routed, 3, "events routed while the channel was on");
    assert_eq!(after_disable, 3, "no event routed once it was off");
}
