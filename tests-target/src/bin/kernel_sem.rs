//! Semaphores on the real kernel: blocking and waking across real context
//! switches, and `give_from_isr` from a real interrupt (EGU0, triggered by
//! a thread writing its TRIGGER task).
#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, Ordering::SeqCst};

use ktest::tests;
use tests_target::EventLog;
use userlib::{sys_sleep, sys_yield, Semaphore};

static LOG: EventLog<64> = EventLog::new();

static MUTEX: Semaphore = kernel::semaphore!(1);
static SHARED: AtomicU32 = AtomicU32::new(0);
static GO_INC: [Semaphore; 3] = [
    kernel::semaphore!(0),
    kernel::semaphore!(0),
    kernel::semaphore!(0),
];
static DONE_INC: Semaphore = kernel::semaphore!(0);

static GATE: Semaphore = kernel::semaphore!(0);
static GO_WAIT: [Semaphore; 3] = [
    kernel::semaphore!(0),
    kernel::semaphore!(0),
    kernel::semaphore!(0),
];

static PAIR: Semaphore = kernel::semaphore!(2);
static GO_GIVER: Semaphore = kernel::semaphore!(0);

static FROM_ISR: Semaphore = kernel::semaphore!(0);
static GO_ISR_WAITER: Semaphore = kernel::semaphore!(0);
static ISR_COUNT: AtomicU32 = AtomicU32::new(0);

// EGU0 (event generator unit): a task a thread can trigger, whose event
// raises the SWI0_EGU0 interrupt.
const EGU0: usize = 0x4001_4000;
const EGU0_TASKS_TRIGGER0: *mut u32 = EGU0 as *mut u32;
const EGU0_EVENTS_TRIGGERED0: *mut u32 = (EGU0 + 0x100) as *mut u32;
const EGU0_INTENSET: *mut u32 = (EGU0 + 0x304) as *mut u32;

#[no_mangle]
pub extern "C" fn user_main() {
    for entry in [
        incrementer0 as kernel::ThreadEntry,
        incrementer1,
        incrementer2,
    ] {
        kernel::os_add_thread(entry, 256, ktest::WORKER_PRIORITY);
    }
    for entry in [waiter0 as kernel::ThreadEntry, waiter1, waiter2] {
        kernel::os_add_thread(entry, 256, 2);
    }
    kernel::os_add_thread(giver, 256, 4);
    kernel::os_add_thread(isr_waiter, 256, 1);

    kernel::irq_attach(kernel::Interrupt::SWI0_EGU0, egu0_handler);
    kernel::irq_set_priority(kernel::Interrupt::SWI0_EGU0, 0x80);
    kernel::irq_enable(kernel::Interrupt::SWI0_EGU0);
    unsafe { EGU0_INTENSET.write_volatile(1) };

    ktest::start(
        "kernel_sem",
        tests![
            a_semaphore_is_a_mutex_across_threads,
            equal_priority_waiters_wake_in_the_order_they_blocked,
            a_counting_semaphore_blocks_only_when_exhausted,
            an_interrupt_wakes_a_waiting_thread,
            a_thread_woken_by_an_interrupt_preempts_a_lower_one,
            interrupt_gives_add_up_while_nobody_waits,
        ],
    );
}

// Helpers ////////////////////////////////////////////////////////////////////

/// Increments SHARED 100 times inside MUTEX, yielding in the middle of each
/// read-modify-write.
fn incrementer(i: usize) -> ! {
    loop {
        GO_INC[i].take();
        for _ in 0..100 {
            MUTEX.with(|| {
                let v = SHARED.load(SeqCst);
                sys_yield();
                SHARED.store(v + 1, SeqCst);
            });
        }
        DONE_INC.give();
    }
}
extern "C" fn incrementer0() -> ! {
    incrementer(0)
}
extern "C" fn incrementer1() -> ! {
    incrementer(1)
}
extern "C" fn incrementer2() -> ! {
    incrementer(2)
}

/// Waits on GATE, then logs its tag.
fn waiter(i: usize) -> ! {
    loop {
        GO_WAIT[i].take();
        GATE.take();
        LOG.push(b'1' + i as u8);
    }
}
extern "C" fn waiter0() -> ! {
    waiter(0)
}
extern "C" fn waiter1() -> ! {
    waiter(1)
}
extern "C" fn waiter2() -> ! {
    waiter(2)
}

/// Gives PAIR once when started (lower priority than the tests).
extern "C" fn giver() -> ! {
    loop {
        GO_GIVER.take();
        LOG.push(b'G');
        PAIR.give();
    }
}

/// Waits (priority 1) for the EGU0 handler's give, then logs.
extern "C" fn isr_waiter() -> ! {
    loop {
        GO_ISR_WAITER.take();
        FROM_ISR.take();
        LOG.push(b'I');
    }
}

extern "C" fn egu0_handler() {
    unsafe { EGU0_EVENTS_TRIGGERED0.write_volatile(0) };
    ISR_COUNT.fetch_add(1, SeqCst);
    kernel::give_from_isr(&FROM_ISR);
}

fn trigger_egu0() {
    unsafe { EGU0_TASKS_TRIGGER0.write_volatile(1) };
}

// Tests //////////////////////////////////////////////////////////////////////

fn a_semaphore_is_a_mutex_across_threads() {
    SHARED.store(0, SeqCst);
    for go in &GO_INC {
        go.give();
    }
    for _ in &GO_INC {
        DONE_INC.take();
    }
    // Without mutual exclusion, the yields in the middle would lose updates.
    assert_eq!(SHARED.load(SeqCst), 300);
}

/// Equal priorities: which waiter `give` should pick among different
/// priorities is known issue 19.
fn equal_priority_waiters_wake_in_the_order_they_blocked() {
    LOG.clear();
    // Waiters (priority 2) run as soon as they are started and block on GATE
    // in this order.
    for i in [2, 0, 1] {
        GO_WAIT[i].give();
    }
    for _ in 0..3 {
        GATE.give(); // the woken waiter preempts us and logs
    }
    assert_eq!(LOG.get(), "312");
}

fn a_counting_semaphore_blocks_only_when_exhausted() {
    LOG.clear();
    PAIR.take();
    PAIR.take();
    LOG.push(b'T');
    GO_GIVER.give(); // lower priority: runs only once we block
    PAIR.take(); // blocks until `giver` gives
    LOG.push(b'T');
    assert_eq!(LOG.get(), "TGT");
    PAIR.give();
    PAIR.give();
}

fn an_interrupt_wakes_a_waiting_thread() {
    let before = ISR_COUNT.load(SeqCst);
    trigger_egu0();
    FROM_ISR.take(); // returns once the handler gave
    assert_eq!(ISR_COUNT.load(SeqCst), before + 1);
}

/// The handler's give pends a switch: the woken higher-priority thread runs
/// as the interrupt returns, before this thread's next instruction.
fn a_thread_woken_by_an_interrupt_preempts_a_lower_one() {
    LOG.clear();
    GO_ISR_WAITER.give(); // it runs at once and blocks on FROM_ISR
    trigger_egu0();
    // The interrupt is taken within a few instructions of the trigger.
    unsafe { core::arch::asm!("dsb", "isb") };
    LOG.push(b'T');
    assert_eq!(LOG.get(), "IT");
}

fn interrupt_gives_add_up_while_nobody_waits() {
    for _ in 0..3 {
        trigger_egu0();
        sys_sleep(1);
    }
    // Three units are waiting; taking them does not block.
    for _ in 0..3 {
        FROM_ISR.take();
    }
}
