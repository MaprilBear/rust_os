//! On-target stress (docs/TESTING.md, tier 3): threads issuing random
//! syscalls while interrupts above SVCall's priority give semaphores from
//! ISRs. Everything random comes from the run's seed, and Renode's virtual
//! time is deterministic, so a seed reproduces a failure;
//! `cargo xtask stress` sweeps seeds and CPU speeds to move the interrupts
//! around.
//!
//! Checked throughout (a violation panics, which fails the test):
//! * a mutex-protected section never has two threads inside;
//! * a counting semaphore never hands out more than its count;
//! * a canary on every stress thread's stack survives every syscall.
//!
//! At the end, after a drain, every semaphore is back to its initial count,
//! and SIGNAL (given by threads and by an ISR at once) still has every unit
//! given and not taken: a lost update blocks a final take, and the test times
//! out. (There is no non-blocking take, so an extra unit goes unnoticed.)
#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering::SeqCst};

use ktest::{tests, Rng};
use userlib::{sys_sleep, sys_yield, Semaphore};

const THREADS: usize = 4;
const TOKENS: u32 = 2;
const RUN_MS: u32 = 1500;

static MUTEX: Semaphore = kernel::semaphore!(1);
static INSIDE: AtomicU32 = AtomicU32::new(0);
static POOL: Semaphore = kernel::semaphore!(2); // TOKENS
static HELD: AtomicU32 = AtomicU32::new(0);
static SIGNAL: Semaphore = kernel::semaphore!(0);

static GO: [Semaphore; THREADS] = [
    kernel::semaphore!(0),
    kernel::semaphore!(0),
    kernel::semaphore!(0),
    kernel::semaphore!(0),
];
static FINISHED: AtomicU32 = AtomicU32::new(0);
static STOP: AtomicBool = AtomicBool::new(false);
static OPS: AtomicU32 = AtomicU32::new(0);
static SIGNALS_TAKEN: AtomicU32 = AtomicU32::new(0);
/// SIGNAL gives by threads (the ISR's are SIGNALS_GIVEN).
static THREAD_GIVES: AtomicU32 = AtomicU32::new(0);

static TIMER_IRQS: AtomicU32 = AtomicU32::new(0);
static SIGNALS_GIVEN: AtomicU32 = AtomicU32::new(0);
static ISR_RNG: AtomicU32 = AtomicU32::new(1);

// TIMER0: the periodic interrupt. EGU0: the software interrupt it pends.
const TIMER0: usize = 0x4000_8000;
const EGU0: usize = 0x4001_4000;
fn reg(addr: usize) -> *mut u32 {
    addr as *mut u32
}

#[no_mangle]
pub extern "C" fn user_main() {
    let entries: [kernel::ThreadEntry; THREADS] = [stress0, stress1, stress2, stress3];
    let priorities = [2, 3, 3, 4];
    for (entry, prio) in entries.into_iter().zip(priorities) {
        kernel::os_add_thread(entry, 512, prio);
    }

    use kernel::Interrupt::{SWI0_EGU0, TIMER0 as TIMER0_IRQ};
    kernel::irq_attach(TIMER0_IRQ, timer0_handler);
    kernel::irq_attach(SWI0_EGU0, egu0_handler);
    // Both above SVCall and SysTick (0xC0): they can land inside a syscall.
    kernel::irq_set_priority(TIMER0_IRQ, 0x40);
    kernel::irq_set_priority(SWI0_EGU0, 0x80);
    kernel::irq_enable(TIMER0_IRQ);
    kernel::irq_enable(SWI0_EGU0);
    unsafe { reg(EGU0 + 0x304).write_volatile(1) }; // INTENSET: TRIGGERED[0]

    ktest::start(
        "stress_kernel",
        tests![random_syscalls_under_interrupts = 20000],
    );
}

// Interrupts /////////////////////////////////////////////////////////////////

fn next_isr_random() -> u32 {
    let mut x = ISR_RNG.load(SeqCst);
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    ISR_RNG.store(x, SeqCst);
    x
}

/// TIMER0 at 1 MHz; the next compare is 200-2000 us away.
fn timer0_start() {
    unsafe {
        reg(TIMER0 + 0x004).write_volatile(1); // STOP
        reg(TIMER0 + 0x504).write_volatile(0); // MODE: timer
        reg(TIMER0 + 0x508).write_volatile(3); // BITMODE: 32
        reg(TIMER0 + 0x510).write_volatile(4); // PRESCALER: 16 MHz / 16
        reg(TIMER0 + 0x540).write_volatile(1000); // CC[0]
        reg(TIMER0 + 0x304).write_volatile(1 << 16); // INTENSET: COMPARE0
        reg(TIMER0 + 0x00C).write_volatile(1); // CLEAR
        reg(TIMER0).write_volatile(1); // START
    }
}

fn timer0_stop() {
    unsafe {
        reg(TIMER0 + 0x308).write_volatile(1 << 16); // INTENCLR
        reg(TIMER0 + 0x004).write_volatile(1); // STOP
    }
}

extern "C" fn timer0_handler() {
    unsafe {
        // Clear EVENTS_COMPARE[0].
        reg(TIMER0 + 0x140).write_volatile(0);
        // Next compare relative to now (CAPTURE[1] -> CC[1]); not the
        // COMPARE0 -> CLEAR shortcut, which Renode's TIMER model ignores.
        reg(TIMER0 + 0x044).write_volatile(1);
        let now = reg(TIMER0 + 0x544).read_volatile();
        reg(TIMER0 + 0x540).write_volatile(now + 200 + next_isr_random() % 1800);
        // Pend the software interrupt (EGU0), as a driver would.
        reg(EGU0).write_volatile(1); // TASKS_TRIGGER[0]
    }
    TIMER_IRQS.fetch_add(1, SeqCst);
}

extern "C" fn egu0_handler() {
    unsafe { reg(EGU0 + 0x100).write_volatile(0) }; // EVENTS_TRIGGERED[0]
    SIGNALS_GIVEN.fetch_add(1, SeqCst);
    kernel::give_from_isr(&SIGNAL);
}

// Stress threads /////////////////////////////////////////////////////////////

/// Words on this thread's stack that nothing may touch.
struct Canary([u32; 8]);

impl Canary {
    fn new(id: usize) -> Self {
        Canary(core::array::from_fn(|i| {
            0xC0DE_0000 | (id as u32) << 8 | i as u32
        }))
    }
    fn check(&self, id: usize) {
        let fresh = Canary::new(id);
        let words = unsafe { core::ptr::read_volatile(&self.0) };
        assert_eq!(words, fresh.0, "thread {id}: stack canary changed");
    }
}

fn stress(id: usize) -> ! {
    loop {
        GO[id].take();
        let canary = Canary::new(id);
        let mut rng = Rng::for_run(id as u32 + 1);
        while !STOP.load(SeqCst) {
            match rng.below(7) {
                0 => sys_yield(),
                1 => sys_sleep(rng.below(3)),
                2 => MUTEX.with(|| {
                    assert_eq!(
                        INSIDE.fetch_add(1, SeqCst),
                        0,
                        "two threads inside the mutex"
                    );
                    if rng.below(2) == 0 {
                        sys_yield();
                    } else {
                        sys_sleep(rng.below(2));
                    }
                    INSIDE.fetch_sub(1, SeqCst);
                }),
                3 => {
                    POOL.take();
                    let held = HELD.fetch_add(1, SeqCst) + 1;
                    assert!(held <= TOKENS, "{held} holders of {TOKENS} tokens");
                    sys_yield();
                    HELD.fetch_sub(1, SeqCst);
                    POOL.give();
                }
                4 => {
                    SIGNAL.take(); // given by the EGU0 handler
                    SIGNALS_TAKEN.fetch_add(1, SeqCst);
                }
                5 => {
                    // Races the EGU0 handler's give on the same semaphore.
                    THREAD_GIVES.fetch_add(1, SeqCst);
                    SIGNAL.give();
                }
                _ => {
                    // Hand the CPU to whoever MUTEX wakes, mid-section.
                    MUTEX.with(sys_yield);
                }
            }
            canary.check(id);
            OPS.fetch_add(1, SeqCst);
        }
        canary.check(id);
        FINISHED.fetch_add(1, SeqCst);
    }
}
extern "C" fn stress0() -> ! {
    stress(0)
}
extern "C" fn stress1() -> ! {
    stress(1)
}
extern "C" fn stress2() -> ! {
    stress(2)
}
extern "C" fn stress3() -> ! {
    stress(3)
}

// The test ///////////////////////////////////////////////////////////////////

fn random_syscalls_under_interrupts() {
    ISR_RNG.store(ktest::seed() | 1, SeqCst);
    THREAD_GIVES.store(0, SeqCst);
    SIGNALS_TAKEN.store(0, SeqCst);
    SIGNALS_GIVEN.store(0, SeqCst);
    STOP.store(false, SeqCst);
    FINISHED.store(0, SeqCst);
    timer0_start();
    for go in &GO {
        go.give();
    }
    sys_sleep(RUN_MS);

    // Stop, then drain: a thread waiting on SIGNAL is released by gives (the
    // timer keeps giving until stopped, and we add some); MUTEX and POOL
    // waiters by their holders.
    STOP.store(true, SeqCst);
    while FINISHED.load(SeqCst) < THREADS as u32 {
        THREAD_GIVES.fetch_add(1, SeqCst);
        SIGNAL.give();
        sys_sleep(5);
    }
    timer0_stop();
    sys_sleep(5); // a last EGU0 interrupt may still be pending

    let ops = OPS.load(SeqCst);
    let irqs = TIMER_IRQS.load(SeqCst);
    ktest::report!(
        "INFO stress_kernel seed={} ops={} timer_irqs={} signals_given={} signals_taken={}",
        ktest::seed(),
        ops,
        irqs,
        SIGNALS_GIVEN.load(SeqCst),
        SIGNALS_TAKEN.load(SeqCst)
    );
    assert!(ops > 1000, "only {ops} operations");
    assert!(irqs > 100, "only {irqs} timer interrupts");
    // Every unit given and not taken is still there (a lost update would
    // block one of these takes).
    let given = SIGNALS_GIVEN.load(SeqCst) + THREAD_GIVES.load(SeqCst);
    for _ in SIGNALS_TAKEN.load(SeqCst)..given {
        SIGNAL.take();
    }
    assert_eq!(INSIDE.load(SeqCst), 0);
    assert_eq!(HELD.load(SeqCst), 0);
    // Every semaphore is back to its count: MUTEX and POOL take without
    // blocking (a blocked take would time this test out).
    MUTEX.take();
    MUTEX.give();
    for _ in 0..TOKENS {
        POOL.take();
    }
    for _ in 0..TOKENS {
        POOL.give();
    }
}
