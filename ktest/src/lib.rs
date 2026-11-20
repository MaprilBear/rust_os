//! `ktest`: the on-target test harness (docs/TESTING.md, tier 2).
//!
//! A suite image's `user_main` calls [`start`] with its tests. That adds a
//! runner thread (priority [`RUNNER_PRIORITY`], highest) and one worker thread
//! per test (priority [`WORKER_PRIORITY`]); `user_main` may add more threads
//! of its own. The runner hands the tests to the workers one at a time and
//! reports each outcome on the debug console, one whole line per event:
//!
//! ```text
//! KTEST START suite=<suite> tests=<n> seed=<s>
//! KTEST RUN <suite>::<test>
//! KTEST PASS <suite>::<test>
//! KTEST FAIL <suite>::<test> <file>:<line> <message>
//! KTEST ABORT <reason>
//! KTEST DONE pass=<n> fail=<n>
//! ```
//!
//! A test fails by panicking (`assert!` and friends): the panic handler
//! reports it and the kernel parks the thread (feature `ktest`), so the next
//! test runs on a fresh worker. A test that does not finish within its
//! timeout fails too. [`expect_fault`] passes when the code it runs takes a
//! MemManage fault (MPU tests); [`expect_fault_on`] also checks its kind
//! (instruction fetch or data access). A fault anywhere else, or a panic in
//! privileged code, prints `KTEST ABORT` and stops the image.
//!
//! The seed for [`Rng`] comes from [`seed`]: the host may write it before the
//! image starts (Renode: `sysbus WriteDoubleWord` at the `KTEST_SEED` symbol).
#![no_std]

use core::fmt::{self, Write};
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering::SeqCst};

use userlib::{sys_sleep, Global, Semaphore};

/// The runner: above every test thread, so it can time them out.
pub const RUNNER_PRIORITY: u8 = 0;
/// Worker threads. Suites may add helper threads above and below.
pub const WORKER_PRIORITY: u8 = 3;
/// Stack of each worker thread, in words.
pub const WORKER_STACK_WORDS: usize = 1024;
pub const DEFAULT_TIMEOUT_MS: u32 = 2000;
/// How often the runner checks on a test.
const POLL_MS: u32 = 2;

/// One test: a function that panics on failure.
pub struct Test {
    pub name: &'static str,
    pub run: fn(),
    pub timeout_ms: u32,
}

/// `tests![a, b, c = 5000]`: a `&'static [Test]` from test functions, with an
/// optional timeout in milliseconds.
#[macro_export]
macro_rules! tests {
    ($($name:ident $(= $timeout:expr)?),* $(,)?) => {
        &[$($crate::Test {
            name: stringify!($name),
            run: $name,
            timeout_ms: $crate::tests!(@timeout $($timeout)?),
        }),*]
    };
    (@timeout) => { $crate::DEFAULT_TIMEOUT_MS };
    (@timeout $t:expr) => { $t };
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Seed and PRNG
///////////////////////////////////////////////////////////////////////////////////////////////////

/// Magic, seed. In `.uninit` (link.ld), which the boot code leaves alone, so a
/// value written by the host before the image starts survives. Without the
/// magic (power-on garbage on hardware), the seed is 1.
#[no_mangle]
#[used]
#[link_section = ".uninit.ktest"]
pub static mut KTEST_SEED: [u32; 2] = [0; 2];
const SEED_MAGIC: u32 = 0x5EED_5EED;

/// The seed for this run (never 0).
pub fn seed() -> u32 {
    let [magic, seed] = unsafe { core::ptr::read_volatile(&raw const KTEST_SEED) };
    if magic == SEED_MAGIC && seed != 0 {
        seed
    } else {
        1
    }
}

/// xorshift32: small, and the same sequence on Renode and hardware.
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Rng(seed.max(1))
    }
    /// A generator for this run, derived from [`seed`] and `stream` (so two
    /// threads can draw independent sequences).
    pub fn for_run(stream: u32) -> Self {
        Rng::new(seed() ^ stream.wrapping_mul(0x9E37_79B9))
    }
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    /// Uniform-ish in `0..n` (n > 0).
    pub fn below(&mut self, n: u32) -> u32 {
        ((self.next_u32() as u64 * n as u64) >> 32) as u32
    }
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Reporting
///////////////////////////////////////////////////////////////////////////////////////////////////

/// One protocol line, built in place and written with a single syscall (the
/// kernel writes it inside one critical section, so nothing can split it).
pub struct Line {
    buf: [u8; 200],
    len: usize,
    truncated: bool,
}

impl Line {
    pub fn new() -> Self {
        let mut l = Line {
            buf: [0; 200],
            len: 0,
            truncated: false,
        };
        l.push_str("KTEST ");
        l
    }
    fn push_str(&mut self, s: &str) {
        for b in s.bytes() {
            if self.truncated {
                return;
            }
            // Keep the newline for the end; leave room for it and "...".
            if self.len + 4 >= self.buf.len() {
                self.buf[self.len..self.len + 3].copy_from_slice(b"...");
                self.len += 3;
                self.truncated = true;
                return;
            }
            match b {
                b'\n' => {
                    self.buf[self.len..self.len + 2].copy_from_slice(b"\\n");
                    self.len += 2;
                }
                b'\r' => {}
                _ => {
                    self.buf[self.len] = b;
                    self.len += 1;
                }
            }
        }
    }
    /// The finished line, with its newline.
    pub fn bytes(&mut self) -> &[u8] {
        self.buf[self.len] = b'\n';
        &self.buf[..self.len + 1]
    }
    /// Send it from a thread (unprivileged): the `DebugWrite` syscall.
    pub fn send(mut self) {
        let b = self.bytes();
        userlib::sys_debug_write(b.as_ptr() as usize, b.len());
    }
}

impl Default for Line {
    fn default() -> Self {
        Self::new()
    }
}

impl Write for Line {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.push_str(s);
        Ok(())
    }
}

/// `report!("PASS {}", name)`: one `KTEST ...` line from a thread.
#[macro_export]
macro_rules! report {
    ($($arg:tt)*) => {{
        let mut line = $crate::Line::new();
        let _ = core::fmt::Write::write_fmt(&mut line, format_args!($($arg)*));
        line.send();
    }};
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Runner and workers
///////////////////////////////////////////////////////////////////////////////////////////////////

// One outcome word per test, so a test that finishes, panics or faults after
// it timed out cannot change another test's outcome. The low byte is the
// state; for FAULTED the kernel puts the fault's MMFSR in bits 24-31.
const RUNNING: u32 = 0;
const PASSED: u32 = 1;
const FAILED: u32 = 2;
/// Written by the kernel when an `expect_fault` fault happens.
const FAULTED: u32 = 3;
/// The panic handler is printing this test's FAIL line.
const REPORTING: u32 = 4;
const STATE: u32 = 0xFF;

/// Tests per image. Each needs a worker stack too ([`WORKER_STACK_WORDS`]),
/// so user RAM runs out at about 50.
pub const MAX_TESTS: usize = 64;

static SUITE: Global<&str> = Global::new("");
static TESTS: Global<&[Test]> = Global::new(&[]);
static CURRENT: AtomicUsize = AtomicUsize::new(0);
/// Written by the kernel too (FAULTED), so plain words in user RAM.
static OUTCOMES: [AtomicU32; MAX_TESTS] = [const { AtomicU32::new(RUNNING) }; MAX_TESTS];
/// The MMFSR bits an `expect_fault_on` test wants (0: any MemManage fault).
static WANT_FAULT: [AtomicU32; MAX_TESTS] = [const { AtomicU32::new(0) }; MAX_TESTS];
/// Per worker: the test it runs, and an address near the top of its stack.
static WORKER_TEST: [AtomicUsize; MAX_TESTS] = [const { AtomicUsize::new(0) }; MAX_TESTS];
static WORKER_SP: [AtomicUsize; MAX_TESTS] = [const { AtomicUsize::new(0) }; MAX_TESTS];
static NEXT_WORKER: AtomicUsize = AtomicUsize::new(0);
static GO: Semaphore = kernel::semaphore!(0);

fn suite() -> &'static str {
    SUITE.read()
}
fn tests() -> &'static [Test] {
    TESTS.read()
}

/// Finish test `test` with `state`, unless it already finished (or timed out).
fn finish(test: usize, state: u32) -> bool {
    OUTCOMES[test]
        .compare_exchange(RUNNING, state, SeqCst, SeqCst)
        .is_ok()
}

/// The test the calling thread runs: found from its stack (worker stacks do
/// not overlap). A helper thread a suite added runs on behalf of the current
/// test.
fn my_test() -> usize {
    let here = 0u8;
    let sp = &here as *const u8 as usize;
    let workers = NEXT_WORKER.load(SeqCst).min(MAX_TESTS);
    (0..workers)
        .find(|&w| {
            let top = WORKER_SP[w].load(SeqCst);
            top >= sp && top - sp < WORKER_STACK_WORDS * 4
        })
        .map_or_else(|| CURRENT.load(SeqCst), |w| WORKER_TEST[w].load(SeqCst))
}

/// Register the runner and one worker per test. Call once from `user_main`.
pub fn start(suite: &'static str, tests: &'static [Test]) {
    assert!(
        tests.len() <= MAX_TESTS,
        "at most {MAX_TESTS} tests per image"
    );
    unsafe {
        *SUITE.as_ptr() = suite;
        *TESTS.as_ptr() = tests;
    }
    kernel::os_add_thread(runner, 1024, RUNNER_PRIORITY);
    for _ in tests {
        kernel::os_add_thread(worker, WORKER_STACK_WORDS, WORKER_PRIORITY);
    }
}

extern "C" fn runner() -> ! {
    let tests = tests();
    report!(
        "START suite={} tests={} seed={}",
        suite(),
        tests.len(),
        seed()
    );
    let (mut pass, mut fail) = (0, 0);
    for (i, t) in tests.iter().enumerate() {
        OUTCOMES[i].store(RUNNING, SeqCst);
        CURRENT.store(i, SeqCst);
        report!("RUN {}::{}", suite(), t.name);
        GO.give();
        let mut waited = 0;
        let outcome = loop {
            sys_sleep(POLL_MS);
            waited += POLL_MS;
            let o = OUTCOMES[i].load(SeqCst);
            match o & STATE {
                // Claim the outcome first, so the test cannot finish now.
                RUNNING if waited >= t.timeout_ms && finish(i, FAILED) => {
                    report!(
                        "FAIL {}::{} - timed out after {} ms",
                        suite(),
                        t.name,
                        t.timeout_ms
                    );
                    break FAILED;
                }
                RUNNING | REPORTING => continue,
                _ => break o,
            }
        };
        let passed = match outcome & STATE {
            PASSED => true,
            FAULTED => {
                let (want, got) = (WANT_FAULT[i].load(SeqCst), outcome >> 24);
                let ok = want == 0 || got & want != 0;
                if !ok {
                    report!(
                        "FAIL {}::{} - expected a MemManage fault with MMFSR bits {:#x}, got MMFSR={:#x}",
                        suite(),
                        t.name,
                        want,
                        got
                    );
                }
                ok
            }
            _ => false, // the FAIL line is already out
        };
        if passed {
            report!("PASS {}::{}", suite(), t.name);
            pass += 1;
        } else {
            fail += 1;
        }
    }
    report!("DONE pass={} fail={}", pass, fail);
    loop {
        sys_sleep(1000);
    }
}

extern "C" fn worker() -> ! {
    let me = NEXT_WORKER.fetch_add(1, SeqCst);
    let top = 0u8;
    WORKER_SP[me].store(&top as *const u8 as usize, SeqCst);
    loop {
        GO.take();
        let i = CURRENT.load(SeqCst);
        WORKER_TEST[me].store(i, SeqCst);
        (tests()[i].run)();
        finish(i, PASSED);
    }
}

/// Which access an [`expect_fault_on`] test expects to fault.
#[derive(Clone, Copy)]
pub enum Access {
    /// An instruction fetch (MMFSR.IACCVIOL), e.g. executing kernel code.
    Instruction,
    /// A load or store (MMFSR.DACCVIOL), e.g. reading kernel RAM.
    Data,
}

/// Run `f`, which must take a MemManage fault (e.g. touch kernel memory).
/// The kernel then parks this thread and the test passes; if `f` returns,
/// the test fails.
#[track_caller]
pub fn expect_fault(f: impl FnOnce()) {
    arm_fault(0, f)
}

/// [`expect_fault`], and the fault must be of the given kind.
#[track_caller]
pub fn expect_fault_on(access: Access, f: impl FnOnce()) {
    let mmfsr_bit = match access {
        Access::Instruction => 1 << 0,
        Access::Data => 1 << 1,
    };
    arm_fault(mmfsr_bit, f)
}

#[track_caller]
fn arm_fault(want: u32, f: impl FnOnce()) {
    let test = my_test();
    WANT_FAULT[test].store(want, SeqCst);
    let armed = userlib::sys_expect_fault(OUTCOMES[test].as_ptr() as usize, FAULTED as usize);
    assert_eq!(
        armed, 0,
        "the kernel refused to arm the expected-fault hook"
    );
    f();
    userlib::sys_expect_fault(0, 0);
    panic!("expected a MemManage fault, but the code returned");
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Panics
///////////////////////////////////////////////////////////////////////////////////////////////////

/// The image's panic handler. In a test thread: report `FAIL` for the test
/// this thread runs and ask the kernel to park the thread. In privileged
/// code: `ABORT`.
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    if !userlib::in_handler_mode() && thread_is_unprivileged() {
        let i = my_test();
        // A test that already finished or timed out keeps its outcome.
        if OUTCOMES[i]
            .compare_exchange(RUNNING, REPORTING, SeqCst, SeqCst)
            .is_ok()
        {
            let name = tests().get(i).map_or("?", |t| t.name);
            let mut line = Line::new();
            let _ = write!(line, "FAIL {}::{} ", suite(), name);
            match info.location() {
                Some(l) => {
                    let _ = write!(line, "{}:{} ", l.file(), l.line());
                }
                None => line.push_str("?:0 "),
            }
            let _ = write!(line, "{}", info.message());
            line.send();
            OUTCOMES[i].store(FAILED, SeqCst);
        }
        userlib::sys_fault();
    }
    abort(format_args!("panic: {}", info))
}

/// Print `KTEST ABORT <why>` from privileged code and stop.
pub fn abort(why: fmt::Arguments) -> ! {
    let mut line = Line::new();
    let _ = write!(line, "ABORT {}", why);
    critical_section::with(|_| {
        for &b in line.bytes() {
            kernel::debugcon::putc(b);
        }
    });
    #[allow(clippy::empty_loop)]
    loop {}
}

/// CONTROL.nPRIV (bit 0). Only meaningful in thread mode.
fn thread_is_unprivileged() -> bool {
    let control: u32;
    unsafe {
        core::arch::asm!("mrs {}, control", out(reg) control, options(nomem, nostack, preserves_flags))
    };
    control & 1 != 0
}
