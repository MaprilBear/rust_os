# Testing

How this RTOS is tested, and the plan for the parts that do not exist yet.
Status is tracked in [Roadmap](#roadmap).

## Goals

- **Kernel**: unit tests, functional tests in Renode, and fuzzing that stresses
  critical sections and kernel memory safety.
- **Drivers**: tests that confirm their critical sections hold on the
  peripherals they control.
- The same target tests run in **Renode and on real hardware** (nRF52840-DK).
- Easy to extend to dynamically loaded processes and more drivers.

## Three tiers

| Tier | Runs on | Tools | Covers |
|---|---|---|---|
| 1. Host | x86 (`cargo xtask test-host`) | unit tests, proptest, loom, Miri | pure logic, data structures, scheduler and semaphore state machines, register layouts, driver logic against fake registers |
| 2. Target | Renode and the DK, same ELF | `ktest` harness + runner (planned) | real context switches, MPU faults, syscalls, ISRs, drivers on peripherals |
| 3. Fuzz / stress | host + target | proptest state machines, cargo-fuzz, seeded stress images | critical sections, memory safety, hostile syscalls, interrupt storms |

## Commands

Run from the repository root (in the dev container):

```
cargo xtask test-host     # tier 1
cargo xtask test-loom     # tier 1, loom model checks (drivers, --cfg loom)
cargo xtask miri          # tier 3, the host tests under Miri (nightly)
cargo xtask fuzz          # tier 3, cargo-fuzz, 60 s of `syscalls` (nightly)
cargo xtask test-renode   # tier 2 in Renode (renode/*.robot, renode/tests/)
cargo xtask test-hw       # tier 2 on an nRF52840-DK (probe-rs, RTT)
cargo xtask test          # test-host, test-loom and test-renode
cargo xtask help          # everything else (build, check-placement, clippy, fmt)
```

`make test`, `make test-host` and `make test-renode` run the same commands in
Docker from outside the container.

Plain `cargo test` does **not** run the host tests: the workspace's default
target is the firmware (thumbv7em, no `std`) and its default member is `app`,
so it finds no tests and reports success. `test-host` adds
`--target host-tuple -p kernel -p userlib -p drivers`. To run some tests:
`cargo xtask test-host -- <name filter>` (args after `--` go to the test
binaries, e.g. `--nocapture`).

**Editor support**: the dev container points rust-analyzer at the firmware
target, so `#[cfg(test)]` modules, `arch::host` and `test_support` are greyed
out. While writing host tests, set `rust-analyzer.cargo.target` to your host
triple (or remove it) in the workspace settings.

## Layout

```
kernel/src/arch/           port layer: armv7m (the real one) and host (a fake, for tests)
kernel/src/arch/armv7m/frame.rs  thread frame layout, shared by both backends
kernel/src/test_support.rs host test API (lock/reset, simulation, fake-port inspection)
userlib/src/host.rs        host stand-ins for svc / IPSR
drivers/                   peripheral drivers (library, host-testable)
app/                       thin firmware binary: user_main + threads
xtask/                     `cargo xtask` (build, placement check, test runners)
renode/*.robot             Renode suites (run by `cargo xtask test-renode`)
kernel/src/model_tests.rs  model-based tests of the scheduler and semaphores
fuzz/                      cargo-fuzz targets (nightly, own workspace)
ktest/                     on-target test harness (no_std)
tests-target/              one firmware image per target suite (src/bin)
renode/tests/              Robot suites for those images (ktest.resource)
```

## Tier 1: host tests

The kernel, `userlib` and `drivers` build for the host. Everything CPU- or
linker-specific is behind `kernel::arch`; on a host, `arch::host` stands in for
the hardware and records what the kernel asked it to do (PendSV, NVIC writes,
log output). Tests live next to the code in `#[cfg(test)] mod tests`.

### Writing a host test

```rust
#[cfg(test)]
mod tests {
    use crate::test_support::*;
    use crate::ThreadState;

    #[test]
    fn sleeping_thread_wakes_after_its_ticks() {
        let _k = lock(); // serialise with other kernel tests, reset all state
        boot(); // the idle thread, as the boot code adds it
        let t = spawn(1); // lower number = higher priority
        assert_eq!(context_switch(), Some(t)); // t is now running
        userlib::sys_sleep(2); // a syscall made by t
        assert_eq!(thread_state(t), Some(ThreadState::Sleeping));
        tick();
        tick();
        assert_eq!(thread_state(t), Some(ThreadState::Ready));
    }
}
```

What `kernel::test_support` gives you (its module docs have the details):

- `lock()`: hold it for the whole test. It resets the scheduler (and frees
  thread stacks), the running thread, the IRQ table, the Start flag, the fake
  port and the userlib hooks. It does **not** reset `semaphore!` objects or
  driver statics: call `register_semaphore(&sem)` for each semaphore a test
  uses. That resets it to its initial count and makes the kernel accept its
  handle (unregistered handles are rejected, as invalid ones are on hardware).
- `spawn(priority)` creates a thread; `context_switch()` does what PendSV does
  and returns the new current thread; `run_pending_switch()` does it only if
  a switch was requested.
- `userlib::sys_*` calls run as calls from the current thread, so a thread must
  be running (`spawn` then `context_switch`). A call that would block returns
  at once and leaves the thread `Blocked`. Syscalls from inside `in_isr` panic
  (on hardware they escalate to HardFault). `dispatch(num, arg, caller)` calls
  the kernel's dispatcher directly, without those checks.
- `tick()` is one SysTick; `in_isr(|| ..)` and `raise_irq(irq)` run code as an
  interrupt handler.
- `pendsv_pending()`, `nvic_*()`, `systick_started()`, `take_log()` inspect
  the fake port. `set_ksem_range`, `set_user_text_range`, `set_flash_range`
  and `set_uram_range` make address checks behave as on hardware (flash/URAM
  matter once a syscall takes a pointer).

### What the host unit tests cover

| Module | Tests |
|---|---|
| `simple_list` | push/remove/retain/index, slot reuse |
| `arch/armv7m/frame` | initial exception frame (PC, LR poison, xPSR) |
| `lib` | thread creation: ids, stack size rounding and alignment, saved SP, user-text check, idle thread |
| `scheduler` | priority preemption, round robin within a level, idle fallback, sleep/tick timing, block/unblock, remove; both policies on their own |
| `semaphore` | counting, blocking, FIFO wake-up, hand-off, preemption by a woken waiter, handle validation, boot callers, `give_from_isr` |
| `syscall` | every syscall number from threads and boot, invalid numbers, `in_range`/`user_can_read`/`user_can_write` |
| `irq` | IRQ numbers, attach/dispatch, enable/priority/unpend, lines above 31 |
| `debugcon`, `test_support` | the log, and the test support itself (reset, bridge checks, `in_isr`) |
| `userlib` | syscall numbers and wrappers, `Semaphore`, `Global` |
| `drivers::fifo` | order, capacity, wrap-around, `pop_into`, two OS threads; loom model of SPSC |
| `drivers::mmio` | `Reg`, `Task`, `Event`, count fields, `Mmio` |
| `drivers::timer`, `ppi` | register maps and field bits against the SVD, base addresses |
| `drivers::uarte` | register map, `init`, TX (DMA, ENDTX wake-up), RX (buffer swap, FIFO, overflow), handler corner cases |

### Tests for known issues

A test for a [known issue](#known-issues-to-confirm-with-tests) asserts the
correct behaviour and is marked `#[ignore = "known issue N: ..."]`, so the
suite stays green until the fix lands. To see them fail:

```
cargo xtask test-host -- --ignored
```

The fix removes the `#[ignore]`.

### Driver tests

Point an `Mmio` at a register block in host RAM and drive the driver's
interrupt handler by hand. This only works on an instance the test builds
itself: `UARTE0`, `uart_init()` and `uarte0_handler()` use the real address
0x4000_2000 and would crash on a host. `Uarte::new` is private, so such tests
live inside `uarte.rs`, and (until known issue 3 is fixed) any instance built
that way shares its semaphores with `UARTE0`. `uart_init` is
`UARTE0.init(irq)`, so the tests run `init` on their own instance (the `Fake`
helper in `uarte.rs`).

Host tests cannot check that `send` holds `regs_semaphore` across a whole
transfer (a blocking `take` returns at once on the host), nor races between
threads calling the driver; those are target tests.

Register maps are checked field by field against the offsets and bit
positions in the nRF52840 SVD (`renode/nrf52840.svd`), not against the
comments in the structs.

**loom** (`cargo xtask test-loom`): the drivers build with `--cfg loom`, which
swaps the FIFO's atomics and slot cell for loom's (`Slot` in `fifo.rs`) and
compiles `uarte` out (its statics need `const` constructors). Models live in
`#[cfg(all(test, loom))] mod loom_tests`; keep them small, and bound
preemptions if exploration takes more than a second or two. Check a new model
by weakening an ordering and watching it fail.

What host tests cannot see: real interrupt preemption (`cpsid i`, PendSV
tail-chaining, an ISR landing inside `schedule()`). That is tier 2/3's job.

## Tier 2: target tests

Tests run as unprivileged threads inside the real kernel, with the real MPU,
in Renode (and, later, on the DK: step 8).

- **`ktest`** (`ktest/`, no_std, user flash): `ktest::start(suite, tests![..])`
  from an image's `user_main` adds a runner thread (priority 0) and one worker
  thread per test (priority 3; a suite may add helper threads of its own,
  above or below). The runner hands the tests to the workers one at a time
  and times each out (default 2 s, `tests![name = ms]`). A test fails by
  panicking; `ktest::expect_fault(|| ..)` passes when the closure takes a
  MemManage fault, and `expect_fault_on(Access::Instruction | Access::Data, ..)`
  also checks its kind (MMFSR); `ktest::Rng` is a seeded xorshift32. Each
  test has its own outcome word, and the panic handler finds the test of the
  thread that panicked from its stack, so a test that panics, faults or
  finishes after it timed out cannot change another test's result. At most
  64 tests per image (`MAX_TESTS`); each worker's 4 KiB stack runs user RAM
  out at about 50.
- **One image per suite** in `tests-target/src/bin/<suite>.rs`, each with its
  own `user_main`, built and placement-checked like the app by
  `cargo xtask test-renode`.
- **Protocol**, line-based and identical on Renode and hardware:

  ```
  KTEST START suite=<suite> tests=<n> seed=<s>
  KTEST RUN <suite>::<test>
  KTEST PASS <suite>::<test>
  KTEST FAIL <suite>::<test> <file>:<line> <message>
  KTEST FAIL <suite>::<test> - timed out after <ms> ms
  KTEST ABORT <reason>
  KTEST DONE pass=<n> fail=<n>
  ```

  The seed comes first so a crash or hang still reports it. Each line is
  written whole with one `DebugWrite`, which the kernel prints inside one
  critical section, and messages escape newlines, so other kernel output
  (`dprintln!` from an ISR, a fault dump) cannot split a line. Lines longer
  than 200 bytes end in `...`.
- **Seed**: `KTEST_SEED` (two words in `.uninit`, which the boot code leaves
  alone) holds a magic and the seed; Renode writes it before starting the
  image (`ktest.resource`, Robot variable `SEED`). Without the magic (power-on
  garbage on hardware) the seed is 1.
- **Kernel hooks**:
  - Syscalls take two arguments (r0-r1) on the target, in the host bridge and
    in userlib.
  - `DebugWrite(ptr, len)` lets unprivileged threads report, at most 256
    bytes (interrupts are off while it prints). The kernel checks the buffer
    with `user_can_read`: user RAM or user flash, never kernel flash
    (host-tested and fuzzed).
  - Feature `ktest` (on for `tests-target` and the host test run of the
    kernel with `--features ktest`): the Fault syscall parks the thread
    instead of halting; `ExpectFault(ptr, value)` arms a one-shot hook for the
    calling thread, and the MemManage handler, for a fault in that thread,
    writes `value` with the MMFSR in bits 24-31 to `ptr` and parks it (never
    for a fault while stacking or unstacking the thread's frame, when its
    stack pointer is somewhere it may not write); other faults (MemManage, HardFault,
    with BusFault and UsageFault escalating to it) print `KTEST ABORT ...`.
    The MemManage report includes the faulting thread's PC.
  - A panic in a test thread prints `FAIL` through `DebugWrite` (the
    `#[panic_handler]` is `ktest`'s), then asks the kernel to park it. A panic
    in privileged code prints `KTEST ABORT panic: ...` and stops.
- **Runner**: `cargo xtask test-renode` builds the app and every suite image
  and passes `KTEST_DIR` to Robot. `renode/tests/ktest.resource` has
  `Run KTest Suite <image>`: boot it, wait for `KTEST DONE` or `KTEST ABORT`,
  and require `fail=0`; the log in Robot's report has every `KTEST` line.
  `renode/tests/ktest_selftest.robot` checks the harness itself against an
  image whose tests pass, fail, hang and fault on purpose.
- **Transport**, by cargo feature: `debugcon` in Renode (the default), or
  the kernel's `rtt` feature on hardware: the console is RTT up channel 0
  (1 KiB, blocking when full, so nothing is lost before the probe attaches;
  it does not depend on the UART driver under test). DK images build with
  `--no-default-features --features rtt`, since a write to 0x6000_0000
  bus-faults on real hardware. Nothing is printed before RAM is initialised
  (`entered reset` is left out under `rtt`). The RTT data lives in kernel RAM;
  the `rtt_target` crate's code is in user flash, like every non-kernel
  crate, and only the kernel calls it. Blocking when full also means an `rtt`
  build of the app stops once 1 KiB is printed if no probe reads it.
- **Hardware** (`cargo xtask test-hw [--seed n] [--chip c] [--probe p]
  [--timeout s] [-- <image>...]`, `xtask/src/hw.rs`): builds every
  tests-target image for RTT with the seed compiled in (`KTEST_SEED`; no
  debugger memory writes), then for each runs `probe-rs run
  --no-catch-hardfault` (so the kernel's handler prints `KTEST ABORT`),
  prints the KTEST lines and stops at `DONE`, `ABORT`, the timeout or
  probe-rs exiting. `hw::IMAGES`
  holds each image's expectation, as the Robot files do for Renode
  (`ktest_selftest` must report exactly `pass=6 fail=8`; `kernel_fpu` and
  `driver_known_issues` are skipped as known issues, and `driver_uarte`
  because nothing drives the UART's other end yet). A unit test fails if an
  image is missing from the table.
  - Setup: an nRF52840-DK on USB (its on-board J-Link/CMSIS-DAP), probe-rs
    (`cargo install --locked probe-rs-tools`; in the Dockerfile), and in the
    dev container access to the probe's USB device (e.g. `--device` or a
    udev rule on the host).
  - Status: built and unit-tested, not yet run on a DK. The RTT transport
    was checked in Renode by reading the RTT buffer out of emulated memory.
  - Next: UART tests through the DK's USB serial port (P0.06 TX / P0.08 RX),
    which first needs `uart_init`'s pins to be configurable (they are P0.00
    and P0.01 today, the DK's 32 kHz crystal: issue 8); and DK-only tests for
    issues 6, 7, 14, 16 and 20.

### Target suites

| Image (`tests-target/src/bin`) | Robot (`renode/tests`) | Covers |
|---|---|---|
| `ktest_selftest` | `ktest_selftest.robot` | the harness: every outcome line, timeouts, expected faults, the seed |
| `kernel_sched` | `kernel.robot` | preemption by a woken higher-priority thread, lower priority waits, equal priorities alternate on yield, waking an equal-priority waiter yields to it, sleep order, r1-r12 across real context switches (with a thread that clobbers them running in between) |
| `kernel_sem` | `kernel.robot` | mutual exclusion across threads (fails without the semaphore), FIFO wake-up among equal priorities, counting, `give_from_isr` from a real interrupt (EGU0), and the thread it wakes preempting a lower one |
| `kernel_syscalls` | `kernel.robot` | Start from a thread, unknown numbers, forged semaphore handles, `DebugWrite`/`ExpectFault` pointer checks |
| `kernel_mpu` | `kernel.robot` | MemManage (data or instruction, as expected) on reading/writing kernel RAM and flash at their edges, forging a semaphore object, reading kernel flash, executing kernel code, writing flash and the vector table, executing from RAM, the debug console; user RAM, flash and peripherals stay accessible |
| `kernel_fpu` | `kernel.robot` (`known-issue`) | floating point in threads (known issue 18) |
| `driver_uarte` | `drivers.robot` | TX (Robot reads UART0), two threads' sends all arrive whole, five lines each (without the TX lock a line is lost: Renode sends a DMA buffer at once, and an ENDTX wake-up reaches the wrong sender), RX of bytes Robot types, a short read leaves the rest queued |
| `driver_timer` | `drivers.robot` | TIMER counter mode through the register map; `regs_semaphore` serialising two threads on a peripheral register (a smoke test of the lock in a driver static) |
| `driver_ppi` | `drivers.robot` | an EGU event routed to a TIMER task by PPI, and none once the channel is off |
| `driver_known_issues` | `drivers.robot` (`known-issue`) | two timers locked at once deadlock (known issue 3) |

Unprivileged access to the System Control Space (NVIC, SysTick, MPU) is a
BusFault on hardware, which escalates to HardFault and stops the system, so
no suite probes it. Renode reports a MemManage fault there instead, which is
why the MPU tests check the kind of fault (`expect_fault_on`): an
"executing kernel code" test must fail on the instruction fetch, not on the
kernel function's first SysTick read.

`--variable MIPS:<n>` (Robot) runs the suites at another CPU speed, which
moves ticks and interrupts relative to the code; timing-sensitive tests start
on a tick (`sys_sleep(1)`) and keep two ticks of margin. They assume a
syscall takes much less than the 1 ms tick: they pass at the default speed
and at 25 and 64 MIPS (the nRF52840 runs at 64 MHz), but not at 5 MIPS and
below in a debug build, where one syscall and switch take milliseconds. Slow
speeds also show known issue 11: with the runner preempting every other tick,
round robin keeps restarting at the head of a level, so `kernel_sched`'s
equal-priority test logs `aaabbb`.

A Robot test of an open known issue is tagged `known-issue`, and one that
needs hardware Renode does not model is tagged `dk-only`.
`cargo xtask test-renode` leaves both out; run them with
`cargo xtask test-renode -- --include known-issue` (or `dk-only`).

Renode differences that shape these suites: the routed PPI task write
happens at the next synchronisation point (so `driver_ppi` sleeps 1 ms after
each event); the TIMER model ignores BITMODE and the COMPARE->CLEAR shortcut,
and a compare reached through COUNT in counter mode is not routed by PPI.
Some UARTE behaviour only the host tests see: the ENDTX interrupt disabling
itself, and the RX handler re-arming the right buffer (Renode stores each
byte at once). Fast RX input is issue 20.

A test that fails inside a driver lock (`regs_semaphore.with(..)`) leaves the
lock held by its parked thread, which blocks every later test that needs it:
read inside the lock, assert outside.

### Writing a target test

```rust
#![no_std]
#![no_main]

use ktest::{expect_fault, tests};

#[no_mangle]
pub extern "C" fn user_main() {
    ktest::start("my_suite", tests![adds, reads_kernel_ram, slow = 5000]);
}

fn adds() {
    assert_eq!(1 + 1, 2);
}

fn reads_kernel_ram() {
    expect_fault(|| unsafe { core::ptr::read_volatile(0x2000_0000 as *const u32); });
}

fn slow() { /* ... */ }
```

Then `renode/tests/my_suite.robot`:

```
*** Settings ***
Suite Setup       Setup
Suite Teardown    Teardown
Test Setup        Reset Emulation
Test Teardown     Test Teardown
Resource          ktest.resource

*** Test Cases ***
My Suite
    Run KTest Suite    my_suite
```

### Generic instances and the kernel/user split

Found while bringing up `ktest`: in debug builds rustc shares generic
instances across crates, so a user crate that depends on `kernel` could call
the kernel's own copy of, say, `core::str::Bytes::into_iter`. link.ld puts
everything from the kernel rlib in KFLASH, so a thread calling it takes a
MemManage fault. rustc shares at opt-levels 0, 1, "s" and "z", so the root
`Cargo.toml` builds the kernel at opt-level 2 in both profiles, and the
placement check fails
any image where user code references a non-kernel symbol defined in a kernel
object (from the linker's cross-reference table, `--cref`).

## Tier 3: fuzzing and stress

- **Model-based (host, proptest; `kernel/src/model_tests.rs`)**: random
  sequences of yield/sleep/take/give from the running thread, ticks and
  `give_from_isr` from interrupts, and context switches run against the real
  kernel and a reference model side by side. Only sequences the hardware can
  produce are generated: idle makes no syscalls, and a thread makes none once
  its own syscall or a tick has requested a switch, but one may still slip in
  after an interrupt above SVCall's priority requested it (late arrival).
  After every step they must agree on every
  thread's state, the current thread, the pending switch, and each
  semaphore's count and wait queue; the invariants (one Running thread, a
  highest-priority Ready thread chosen) are also checked on the kernel
  directly. The model does not predict *which* equal-priority thread runs: it
  checks the kernel picked a valid one and adopts it. Predicting it would mean
  copying the round-robin policy, known issue 11 included, which is why this
  is plain proptest rather than `proptest-state-machine`. One policy-free
  rule is checked: a thread that yields while a Ready peer at its priority
  exists hands the CPU to a peer.
- **Property tests (host, proptest)**: `SimpleList` against a
  `Vec<Option<T>>`, `in_range` against its definition in u128 arithmetic,
  semaphore handle validation around `.ksem`, the initial thread frame, the
  FIFO against a bounded `VecDeque`. They run with the other host tests.
- **Coverage-guided (host, cargo-fuzz; `fuzz/fuzz_targets/syscalls.rs`)**:
  arbitrary syscall numbers and arguments from the running thread
  (semaphore handles valid, misaligned or outside `.ksem`, pointers into a
  host buffer standing in for user RAM, raw integers), interleaved with
  ticks, interrupt gives and switches. The harness takes
  `test_support::lock()` per input, declares its semaphore array as `.ksem`
  (so handles go through the real range/alignment check), sets the user RAM
  range to the buffer (so `DebugWrite` can print from it, and must read only
  inside it),
  skips `Fault` (which halts by design), and checks the scheduler and
  semaphore invariants after every step; anything else must never panic. `cargo xtask fuzz [<target>] [--time <s>]`; crashes
  are saved in `fuzz/artifacts/`, replay one with
  `cargo +nightly fuzz run --fuzz-dir fuzz syscalls <file>`.
- **Undefined behaviour (host, Miri; `cargo xtask miri`)**: the host tests
  of all three crates under Miri. Proptest tests use 8 cases and no
  regression files under `cfg(miri)`; slow tests with no unsafe code are
  skipped there (`#[cfg_attr(miri, ignore = ...)]`). Tests that run into
  known issue 9 (UB) or leak through known issue 2 are skipped the same way
  until those are fixed. Miri stops at the first UB, so look at one at a
  time: `cargo xtask miri -- --exact <test path> --include-ignored`.
- **On-target stress (`tests-target/src/bin/stress_kernel.rs`,
  `renode/tests/stress.robot`)**: four threads at priorities 2-4 issue random
  syscalls (yield, sleep, a mutex section, a counting semaphore, taking and
  giving a signal) while TIMER0, at a seed-chosen random period, pends a
  software interrupt (EGU0) whose handler gives the same signal; both
  interrupts are above SVCall's priority, so they land inside syscalls.
  Checked throughout: no two threads in the mutex section, no more holders
  than the counting semaphore allows, a canary on each thread's stack after
  every syscall. At the end, after a drain: every semaphore is back to its
  count, and the signal still holds every unit given and not taken (a lost
  update blocks a final take and the test times out; with no non-blocking
  take, an extra unit goes unnoticed). `cargo xtask test-renode`
  runs seed 1; `cargo xtask stress [--seeds n] [--first s] [--mips list]`
  sweeps seeds and CPU speeds (default 20 seeds at default, 10 and 50 MIPS)
  in one Robot run and reports each failing pair. Virtual time is
  deterministic, so a seed and speed reproduce a failure. (Making
  `ksem_give` run outside its critical section was caught by 2 of the 18
  runs of a 6-seed sweep, which is why the sweep exists.)

Miri and cargo-fuzz need nightly. The dev container has it next to stable
(stable stays the default toolchain), with the `miri` component and
`cargo-fuzz`.

## The tools

- **proptest**: property-based testing. You state a property that must hold
  for all inputs; proptest generates hundreds of inputs and, on failure,
  shrinks to a small failing case. It records the failing case's seed in
  `<crate>/proptest-regressions/` and replays it on every later run; commit
  those files. `proptest-state-machine` generates operation sequences checked
  against a reference model. Stable Rust; runs with the other host tests.
- **loom**: runs a concurrent test under the thread interleavings and
  memory-model reorderings it can explore (bounded, and SeqCst is modelled
  conservatively), so wrong atomic orderings show up deterministically. For
  small lock-free structures such as the SPSC `Fifo`. The code swaps in loom's
  atomics and `UnsafeCell` under `cfg(loom)`; loom's types are not `const fn`,
  so `Fifo::new` and `LockFreePolicy::new` need a non-const path for loom
  builds.
- **Miri**: an interpreter for Rust's MIR that checks every memory access:
  use-after-free, out-of-bounds, uninitialised reads, aliasing violations
  (two `&mut`), invalid values, data races. Runs the existing tests, about
  50-100x slower. Cannot run inline asm or touch MMIO. Proptest tests need
  fewer cases and no regression-file writes under Miri (`cfg(miri)`), or
  `-Zmiri-disable-isolation`. Nightly.
- **cargo-fuzz**: coverage-guided fuzzing with libFuzzer. Mutates raw input
  bytes, keeps inputs that reach new code, and saves any input that crashes.
  Best for hostile interfaces such as the syscall surface. Nightly; run on
  demand or nightly in CI, not on every build.

## CI

`.github/workflows/ci.yml` runs everything inside the dev-container image
(the Dockerfile, built with the GitHub Actions cache), so CI has the same
toolchains, Renode and Robot as the dev container:

- **Every push and pull request** (`checks`): `fmt --check`,
  `clippy -- -D warnings`, `test-host`, `test-loom`, debug and release
  firmware builds (placement-checked) and `test-renode`. Robot's results are
  also converted to JUnit (`rebot --xunit`) and uploaded with the reports.
- **Nightly and on demand** (`nightly`): `miri`, ten minutes of `fuzz` (the
  corpus is cached between runs) and the `stress` sweep, each run even if an
  earlier one failed; fuzz crashes and Renode reports are uploaded on
  failure.

The Dockerfile's base image (`rust:1-trixie`) is not pinned, so when it
updates the image cache misses and CI rebuilds the image (Renode, the
nightly toolchain, cargo-fuzz and probe-rs), which takes a while.

The hardware runner needs a DK, so it is not in CI; a self-hosted runner
with the board attached could run `cargo xtask test-hw`.

## Roadmap

Every test step starts with a list of test cases for review; code follows
approval.

1. [x] Make kernel, userlib and drivers build and test on the host
   (`arch` layer, `test_support`), split out `drivers`, add `cargo xtask`.
2. [x] Kernel host unit tests
3. [x] Kernel model-based fuzzing (+ Miri, cargo-fuzz targets, nightly in the image)
4. [x] `ktest` harness and runner, in Renode
5. [x] Kernel target suites (scheduling, semaphores, syscalls, MPU attacks)
6. [x] Driver suites (host + target)
7. [x] On-target stress fuzzing
8. [x] Hardware runner (`cargo xtask test-hw`, probe-rs + RTT); not yet run on a DK
9. [x] CI (GitHub Actions, using the Dockerfile)

## Known issues to confirm with tests

Found while reading, refactoring or reviewing the code. Each gets a failing
test before it is fixed. The tag says where that test can run.

1. **[host]** `SimpleList::remove` on an empty slot still decrements `length`:
   a panic with overflow checks (debug, tests), a silent wrap in release.
2. **[host]** `remove_thread` leaves the thread's id in the sleep list and in
   semaphore wait queues, and ids are reused, so the wrong thread can be woken.
   Removing the running thread also leaves `CURR_TCB` dangling, and
   `remove_thread` never frees the thread's stack. (No caller yet; matters
   for dynamic processes.)
3. **[host]** `semaphore!` inside the `const fn` constructors gives every
   instance one kernel object: `UARTE0` and `UARTE1` share both semaphores
   (the map's `.ksem` holds only two `Uarte::new::KOBJ` objects), and the
   timers would share one.
4. **[target, stress]** `Uarte::receive` takes no lock, but its FIFO is single-consumer:
   two threads calling it race inside `pop`. The race needs a preemption
   between `pop`'s load and store of `tail`. A stress test (two readers, a
   2000-byte typed stream) could not show it in Renode: see issue 20.
5. **[host]** `Timer` and `Ppi` expose `pub regs`, which bypasses their
   `regs_semaphore`, contrary to their `Sync` justification.
6. **[DK]** UARTE TX from a flash buffer (e.g. the `"Bad input"` literal in
   `app`): EasyDMA can only read RAM, so this likely fails on hardware while
   working in Renode.
7. **[DK]** UARTE RX double buffering: `RXD.PTR` is latched at STARTRX, so it
   should be updated on RXSTARTED. With the ENDRX→STARTRX shortcut the old
   buffer is re-armed before the ISR swaps pointers, and the extra manual
   STARTRX restarts reception.
8. **[DK]** UARTE enables hardware flow control with CTS disconnected
   (unverified effect; the comment says "no RTS/CTS"), and uses P0.00/P0.01,
   the 32 kHz crystal pins on the DK.
9. **[Miri]** `CURR_TCB`/`NEXT_TCB` are raw pointers into the scheduler's
   thread list while `scheduler()` hands out `&'static mut`. Confirmed: Miri
   reports UB under both Stacked and Tree Borrows. Under Stacked Borrows,
   `NEXT_TCB` is derived from `&mut` to a list element and the next
   `tcb_list[..].as_mut()` (a Unique retag of the whole buffer) invalidates
   it; under Tree Borrows, a shared read of the list (`SimpleList::iter`)
   freezes its tag and the next write through `CURR_TCB` is UB. The 11 tests
   that reach it are skipped under Miri until it is fixed.
10. **Cleanups (no failing test possible):** `Fifo::new` fills its buffer with
    `mem::zeroed()` inside a `const` block, which is a compile error (not UB)
    for types where all-zeroes is invalid; use `MaybeUninit<T>`. PendSV loads
    `NEXT_TCB` and stores `CURR_TCB` twice. The `Interrupt` enum and
    `NUM_IRQS` are nRF52840 data in the portable `irq.rs`. `cargo xtask` must
    run from the repository root.
11. **[host]** `PriorityPolicy::pick_next` restarts a level's round robin at
    its first thread whenever the previous thread ran at another level. A
    higher-priority thread that wakes every tick hands the CPU back to the
    same lower-priority thread each time, and its peers starve.
12. **[host]** An invalid syscall number returns 0, which callers read as
    success; other failures return 1.
13. **[host]** `os_add_thread` with a priority above 7 panics on an
    out-of-bounds index in the policy, after the TCB was added to the thread
    list, so a half-added thread remains. It should be rejected up front with
    a clear message.
14. **[host]** `UarteRegs` swaps PSEL.TXD and PSEL.CTS: the nRF52840 has TXD
    at 0x50C and CTS at 0x510. `uart_init` therefore connects CTS (with flow
    control on, see 8) to P0.00 and leaves TXD disconnected, so nothing is
    transmitted on hardware. Renode's UARTE model ignores PSEL.
15. **[host]** `Uarte::send` saturates TXD.MAXCNT (16 bits), so a buffer over
    65535 bytes is silently cut short. Splitting or rejecting it is an API
    decision; today's behaviour is pinned by a test.
16. **[DK]** `Uarte::send(&[])` starts a zero-length transfer and waits for
    ENDTX. If the UARTE raises no ENDTX for MAXCNT = 0, the thread blocks
    forever.
17. **Cleanup:** TIMER0-2 have four CC registers (TIMER3/4 have six), but
    `TimerRegs` gives every instance `cc4`/`cc5` and `compare4`/`compare5`.
18. **[target]** Nothing enables the FPU (CPACR), so the first floating-point
    instruction a thread executes raises a UsageFault (NOCP), which escalates
    to HardFault and stops the system. On this hard-float target that is any
    `f32`/`f64` arithmetic. Enabling it also needs PendSV to save s16-s31 and
    to use the FP-aware EXC_RETURN (`kernel_fpu`, tagged `known-issue`).
19. **[host]** A semaphore `give` wakes the oldest waiter; it should wake the
    highest-priority one (FIFO among equal priorities). Tests that rely on
    the order use waiters of equal priority until it is fixed.
20. **[DK]** Unverified: in Renode, a fast RX stream (2000 bytes typed at
    once) overwrote the static that follows `UARTE0` with typed bytes, i.e.
    RX DMA wrote past the driver's 1-byte buffers (`RXD.MAXCNT = 1`).
    Hardware honours MAXCNT, so this is probably Renode's UARTE model,
    perhaps together with issue 7's re-arming. Check on the DK; until then,
    Renode tests type short inputs.
