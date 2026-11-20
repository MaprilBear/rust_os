//! Scheduling on the real kernel: real context switches (PendSV), real
//! SysTick. Helper threads wait on their own semaphore until a test starts
//! them, at priorities above (1), equal to (3) and below (5) the test workers.
#![no_std]
#![no_main]

use core::arch::asm;
use core::sync::atomic::{AtomicU32, Ordering::SeqCst};

use ktest::tests;
use tests_target::EventLog;
use userlib::{sys_sleep, sys_yield, Semaphore};

static LOG: EventLog<64> = EventLog::new();

static GO_HIGH: Semaphore = kernel::semaphore!(0);
static GO_LOW: Semaphore = kernel::semaphore!(0);
static GO_PEER_A: Semaphore = kernel::semaphore!(0);
static GO_PEER_B: Semaphore = kernel::semaphore!(0);
static GO_SLEEP: [Semaphore; 3] = [
    kernel::semaphore!(0),
    kernel::semaphore!(0),
    kernel::semaphore!(0),
];
static GO_CLOBBER: Semaphore = kernel::semaphore!(0);
/// Turns `clobber` has taken.
static CLOBBER_TURNS: AtomicU32 = AtomicU32::new(0);

#[no_mangle]
pub extern "C" fn user_main() {
    kernel::os_add_thread(high, 256, 1);
    kernel::os_add_thread(low, 256, 5);
    kernel::os_add_thread(peer_a, 256, ktest::WORKER_PRIORITY);
    kernel::os_add_thread(peer_b, 256, ktest::WORKER_PRIORITY);
    kernel::os_add_thread(sleeper0, 256, 2);
    kernel::os_add_thread(sleeper1, 256, 2);
    kernel::os_add_thread(sleeper2, 256, 2);
    kernel::os_add_thread(clobber, 256, ktest::WORKER_PRIORITY);
    ktest::start(
        "kernel_sched",
        tests![
            higher_priority_thread_runs_as_soon_as_it_is_woken,
            lower_priority_thread_waits_until_the_cpu_is_free,
            waking_an_equal_priority_thread_yields_to_it,
            equal_priorities_take_turns_on_yield,
            sleepers_wake_in_order_of_their_deadlines,
            sleep_zero_returns,
            registers_survive_context_switches,
        ],
    );
}

// Helpers ////////////////////////////////////////////////////////////////////

extern "C" fn high() -> ! {
    loop {
        GO_HIGH.take();
        LOG.push(b'H');
    }
}

extern "C" fn low() -> ! {
    loop {
        GO_LOW.take();
        LOG.push(b'L');
    }
}

fn peer(go: &Semaphore, tag: u8) -> ! {
    loop {
        go.take();
        for _ in 0..3 {
            LOG.push(tag);
            sys_yield();
        }
    }
}
extern "C" fn peer_a() -> ! {
    peer(&GO_PEER_A, b'a')
}
extern "C" fn peer_b() -> ! {
    peer(&GO_PEER_B, b'b')
}

fn sleeper(i: usize, ms: u32, tag: u8) -> ! {
    loop {
        GO_SLEEP[i].take();
        sys_sleep(ms);
        LOG.push(tag);
    }
}
// Deadlines two ticks apart, so a tick landing anywhere cannot reorder them.
extern "C" fn sleeper0() -> ! {
    sleeper(0, 12, b'3')
}
extern "C" fn sleeper1() -> ! {
    sleeper(1, 4, b'1')
}
extern "C" fn sleeper2() -> ! {
    sleeper(2, 8, b'2')
}

/// Fills the registers a context switch must save with junk, then yields.
extern "C" fn clobber() -> ! {
    loop {
        GO_CLOBBER.take();
        // More turns than the test yields: `give` itself already hands it one.
        for _ in 0..8 {
            CLOBBER_TURNS.fetch_add(1, SeqCst);
            unsafe {
                asm!(
                    "push {{r4-r11}}",
                    "mvn r4, #0", "mvn r5, #0", "mvn r6, #0", "mvn r7, #0",
                    "mvn r8, #0", "mvn r9, #0", "mvn r10, #0", "mvn r11, #0",
                    "mvn r1, #0", "mvn r2, #0", "mvn r3, #0", "mvn r12, #0",
                    "svc #1", // Yield
                    "pop {{r4-r11}}",
                    out("r0") _, out("r1") _, out("r2") _, out("r3") _, out("r12") _,
                )
            };
        }
    }
}

// Tests //////////////////////////////////////////////////////////////////////

fn higher_priority_thread_runs_as_soon_as_it_is_woken() {
    LOG.clear();
    GO_HIGH.give(); // switches to `high` before returning
    LOG.push(b'T');
    assert_eq!(LOG.get(), "HT");
}

fn lower_priority_thread_waits_until_the_cpu_is_free() {
    LOG.clear();
    GO_LOW.give();
    LOG.push(b'T');
    sys_yield(); // still ahead of `low`
    LOG.push(b'T');
    sys_sleep(2); // now it can run
    assert_eq!(LOG.get(), "TTL");
}

/// Not only a higher-priority waiter: `give` asks for a switch whenever it
/// wakes someone, and round robin then moves on to the next peer.
fn waking_an_equal_priority_thread_yields_to_it() {
    sys_sleep(1); // start on a tick, so none lands in the middle
    LOG.clear();
    GO_PEER_A.give();
    LOG.push(b'T');
    sys_sleep(5); // let it finish its turns
    let log = LOG.get();
    // `a` ran before give() returned. (How many turns it took first depends
    // on round robin, which known issue 11 perturbs.)
    assert!(
        log.as_str().starts_with('a') && log.as_str().contains('T'),
        "{log:?}"
    );
}

fn equal_priorities_take_turns_on_yield() {
    sys_sleep(1); // start on a tick (a runner preemption would trigger issue 11)
    LOG.clear();
    GO_PEER_A.give();
    GO_PEER_B.give();
    sys_sleep(5);
    let log = LOG.get();
    let log = log.as_str();
    assert_eq!(log.matches('a').count(), 3, "{log}");
    assert_eq!(log.matches('b').count(), 3, "{log}");
    // While both are Ready (from b's first turn to a's last), each yield
    // hands the CPU to the other.
    let (first_b, last_a) = (log.find('b').unwrap(), log.rfind('a').unwrap());
    assert!(first_b < last_a, "they never shared the CPU: {log}");
    let both = &log[first_b..=last_a];
    assert!(!both.contains("aa") && !both.contains("bb"), "{log}");
}

fn sleepers_wake_in_order_of_their_deadlines() {
    sys_sleep(1); // start on a tick
    LOG.clear();
    for s in &GO_SLEEP {
        s.give(); // each (priority 2) starts its sleep at once
    }
    sys_sleep(6); // deadlines 4, 8, 12
    assert_eq!(LOG.get(), "1");
    sys_sleep(4);
    assert_eq!(LOG.get(), "12");
    sys_sleep(4);
    assert_eq!(LOG.get(), "123");
}

fn sleep_zero_returns() {
    for _ in 0..3 {
        sys_sleep(0);
    }
}

/// r1-r3, r12 come back from the hardware-stacked frame, r4-r11 from the
/// kernel's PendSV save area; r0 holds the syscall result.
fn registers_survive_context_switches() {
    GO_CLOBBER.give();
    let mut bad: u32;
    for _ in 0..4 {
        let turns = CLOBBER_TURNS.load(SeqCst);
        unsafe {
            asm!(
                "push {{r4-r11}}",
                "mov r1, #0x11111111", "mov r2, #0x22222222", "mov r3, #0x33333333", "mov r4, #0x44444444", "mov r5, #0x55555555", "mov r6, #0x66666666", "mov r7, #0x77777777", "mov r8, #0x88888888", "mov r9, #0x99999999", "mov r10, #0xaaaaaaaa", "mov r11, #0xbbbbbbbb", "mov r12, #0xcccccccc",
                "svc #1", // Yield: `clobber` runs before this returns
                "movs r0, #0",
                "cmp r1, #0x11111111", "it ne", "addne r0, #1",
                "cmp r2, #0x22222222", "it ne", "addne r0, #1",
                "cmp r3, #0x33333333", "it ne", "addne r0, #1",
                "cmp r4, #0x44444444", "it ne", "addne r0, #1",
                "cmp r5, #0x55555555", "it ne", "addne r0, #1",
                "cmp r6, #0x66666666", "it ne", "addne r0, #1",
                "cmp r7, #0x77777777", "it ne", "addne r0, #1",
                "cmp r8, #0x88888888", "it ne", "addne r0, #1",
                "cmp r9, #0x99999999", "it ne", "addne r0, #1",
                "cmp r10, #0xaaaaaaaa", "it ne", "addne r0, #1",
                "cmp r11, #0xbbbbbbbb", "it ne", "addne r0, #1",
                "cmp r12, #0xcccccccc", "it ne", "addne r0, #1",
                "pop {{r4-r11}}",
                out("r0") bad, out("r1") _, out("r2") _, out("r3") _, out("r12") _,
            )
        };
        assert_eq!(bad, 0, "{bad} registers changed across a context switch");
        assert!(
            CLOBBER_TURNS.load(SeqCst) > turns,
            "`clobber` did not run in between"
        );
    }
}
