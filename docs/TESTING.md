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
cargo xtask test-renode   # tier 2 in Renode (today: renode/*.robot)
cargo xtask test          # all of the above
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

planned:
ktest/                     on-target test harness (no_std)
tests-target/              one firmware image per target suite
renode/tests/              Robot suites for those images
fuzz/                      cargo-fuzz targets (nightly)
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

## Tier 2: target tests (planned)

- **`ktest`** (no_std, user flash): test registration, assertions, timeouts, a
  seeded PRNG and a reporter. Tests run as unprivileged threads inside the real
  kernel, with the real MPU.
- **One image per suite** in `tests-target`, each with its own `user_main`,
  built and placement-checked like the app.
- **Protocol**, line-based and identical on Renode and hardware:

  ```
  KTEST START suite=<suite> tests=<n> seed=<s>
  KTEST RUN <suite>::<test>
  KTEST PASS <suite>::<test>
  KTEST FAIL <suite>::<test> <file>:<line> <message>
  KTEST ABORT <reason>
  KTEST DONE pass=<n> fail=<n>
  ```

  The seed comes first so a crash or hang still reports it. Each line is
  written whole, inside one critical section, and messages escape newlines, so
  other kernel output (`dprintln!` from an ISR, a fault dump) cannot split a
  line. The runner reads only lines starting with `KTEST ` and treats a missing
  `DONE` within the timeout as a failure.
- **Transport**, chosen by cargo feature: the debug console in Renode; RTT
  through probe-rs on the DK (it does not depend on the UART driver under
  test). DK images build without `debugcon`, since a write to 0x6000_0000
  bus-faults on real hardware. RTT must block when its buffer is full
  (or the runner must attach before reset), otherwise early lines are lost.
- **Kernel hooks** (feature `ktest`):
  - The syscall ABI grows from one argument (r0) to two (r0-r1) on the target,
    in the host bridge and in userlib.
  - `DebugWrite(ptr, len)` lets unprivileged test threads report. Its pointer
    check (`user_can_read`) is itself a test and fuzz target.
  - Today a panic in a thread halts the whole image and its message is lost
    (unprivileged code cannot print). Under `ktest`, the panic handler sends a
    `FAIL` line through `DebugWrite`, the Fault syscall parks the thread
    instead of halting, and the remaining fault handlers (HardFault,
    BusFault, UsageFault) print `KTEST ABORT`.
  - An expected-fault hook in the MemManage handler reports the fault and
    parks the thread, so MPU tests can continue.
- **Runners**: the existing `cargo xtask test-renode` gains the suite images;
  a new `cargo xtask test-hw` drives probe-rs. Robot already writes
  `robot_output.xml`; JUnit XML for CI would come from `rebot --xunit`.
- **Hardware**: nRF52840-DK with its on-board debugger. UART tests use the DK's
  USB serial port (P0.06 TX / P0.08 RX), which first needs `uart_init`'s pins
  to be configurable.

## Tier 3: fuzzing and stress (planned)

- **Model-based (host, proptest-state-machine)**: random sequences of
  add/sleep/tick/take/give/schedule run against the real kernel and a
  reference model. Invariants include: exactly one Running thread, the
  highest-priority Ready thread is chosen, no thread is lost, semaphore counts
  are conserved, waiters wake in FIFO order.
- **Coverage-guided (host, cargo-fuzz)**: arbitrary syscall numbers, handles
  and pointers into the dispatcher. The harness takes `test_support::lock()`
  per input, sets the user RAM range to a host buffer (otherwise every pointer
  is rejected and nothing is covered), and treats `Fault` (which halts by
  design) and `Start` specially; for everything else the kernel must never
  panic or corrupt state.
- **Undefined behaviour (host, Miri)**: the host tests re-run under Miri.
- **On-target stress**: N threads issuing random syscalls while a timer ISR
  pends software interrupts that call `give_from_isr`; stack canaries and
  invariant checks throughout. In Renode, sweep seeds and CPU quantum/MIPS to
  shift interrupt timing. Virtual time is deterministic, so a seed reproduces a
  failure.

Miri and cargo-fuzz need nightly. Step 3 adds it to the dev container next to
stable (stable stays the default toolchain).

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

## Roadmap

Every test step starts with a list of test cases for review; code follows
approval.

1. [x] Make kernel, userlib and drivers build and test on the host
   (`arch` layer, `test_support`), split out `drivers`, add `cargo xtask`.
2. [x] Kernel host unit tests
3. [ ] Kernel model-based fuzzing (+ Miri, cargo-fuzz targets, nightly in the image)
4. [ ] `ktest` harness and runner, in Renode
5. [ ] Kernel target suites (scheduling, semaphores, syscalls, MPU attacks)
6. [ ] Driver suites (host + target) (host part done)
7. [ ] On-target stress fuzzing
8. [ ] Hardware runner (`cargo xtask test-hw`, probe-rs + RTT)
9. [ ] CI (GitHub Actions, using the Dockerfile)

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
4. **[target]** `Uarte::receive` takes no lock, but its FIFO is single-consumer:
   two threads calling it race inside `pop`.
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
   thread list while `scheduler()` hands out `&'static mut`; Stacked Borrows
   will likely flag a second context switch.
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
19. **[host]** A semaphore `give` wakes the oldest waiter; it should wake the
    highest-priority one (FIFO among equal priorities). Tests that rely on
    the order use waiters of equal priority until it is fixed.
