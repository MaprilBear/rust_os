# Renode for this project (nRF52840 / Cortex-M4F, Rust)

This is a practical tutorial for using Renode with the kernel in this repo. It
assumes you can already write the bring-up code; it covers the simulator.

## 1. Mental model

Renode simulates a whole machine, not just a CPU. Four concepts cover almost
everything:

| Concept | What it is | In this repo |
|---|---|---|
| **Platform** (`.repl`) | Text description of the SoC: CPU, memory regions, peripherals, their addresses and interrupt lines | `renode/nrf52840.repl` (includes Renode's stock `platforms/cpus/nrf52840.repl`) |
| **Script** (`.resc`) | A list of Monitor commands: create machine, load platform, load ELF, start | `renode/common.resc`, `run.resc`, `debug.resc` |
| **Monitor** | Renode's interactive shell for inspecting and controlling the running machine | the `(nrf52840)` prompt you get from `make run` |
| **Virtual time** | Simulated time that only advances while the CPU runs; it stops when you pause or hit a breakpoint | n/a, but it is why debugging is "transparent" |

Your kernel is loaded as a plain ELF, the same binary you will flash later.

## 2. Project layout

```
Dockerfile              Rust + Renode 1.17.0 (portable) + QEMU + gdb-multiarch
Makefile                all commands run inside the container as your user
link.ld                 nRF52840 map: flash 1M @ 0x0, RAM 256K @ 0x20000000
renode/
  nrf52840.repl         platform (stock nRF52840 + room for your additions)
  common.resc           create machine, load platform + ELF, show UART0
  run.resc              common + start
  debug.resc            common + GDB server on :3334 (CPU held for debugging)
  run.sh                cargo runner (used by `cargo run`)
  test.robot            Robot Framework test
```

## 3. First run

```
make build
make run          # run these in a terminal inside the dev container
```

You will see Renode's log lines (it prints a lot at startup), then a prompt like
`(nrf52840)`. UART0 output appears in that log, because headless mode routes
UART analyzers to the log. Type `quit` to leave.

The stub kernel just spins in `Reset`, so nothing prints yet. Useful sanity
check in the Monitor:

```
(nrf52840) pause
(nrf52840) sysbus.cpu PC
(nrf52840) sysbus.cpu SP
```

After your bring-up exists, `PC` should sit inside your code and `SP` should
equal the first word of your vector table (`_estack` in `link.ld`).

## 4. Monitor cheat sheet

Run `help` for everything; these are the ones you will use most.

```
start / pause / quit              control the simulation
machine Reset                     reset the machine (reloads SP/PC from the vector table)
peripherals                       tree of every peripheral and its address
sysbus WhatPeripheralIsAt 0x40002000
sysbus ReadDoubleWord 0x40002000  read a 32-bit register
sysbus WriteDoubleWord 0x20000000 0x1234
sysbus.cpu PC                     show a CPU register (also SP, LR, R0..R12)
sysbus.cpu Step                   execute one instruction (when paused)
sysbus.cpu ExecutedInstructions
sysbus LogPeripheralAccess sysbus.uart0   log every register access to UARTE0
uart_connect sysbus.uart0         interactive UART session (press ESC to leave)
```

Two behaviours worth knowing:

- Accesses to addresses with no peripheral are logged as warnings (for example
  "read from non-existing peripheral"). If your driver "does nothing", check the
  log for these first; it usually means the peripheral is not modelled or you
  have the wrong address.
- Because virtual time stops when the CPU is paused, a paused machine can be
  inspected indefinitely and resumes exactly where it was.

## 5. UART output (your main debug channel)

On the nRF52840, UART0 in the Renode platform is the **UARTE** with EasyDMA
(`easyDMA: true`) at `0x40002000`. A driver must use the EasyDMA registers:
set `TXD.PTR` and `TXD.MAXCNT`, trigger `TASKS_STARTTX`, then wait for
`EVENTS_ENDTX`. Remember the buffer must be in RAM, not flash.

**Quick alternative: the debug console.** If you just want to print while
debugging and don't want to write a UART driver yet, use `dprintln!` (see
`PROJECT_SETUP.md`, Step 12). It writes to a made-up Renode peripheral and the
text appears in the log as `debugcon: <line>`, with no UART setup. The rest of
this section is about the real UARTE.

Three ways to read it:

1. **Log (default here).** `showAnalyzer sysbus.uart0` in `common.resc` makes
   UART0 output appear in the Renode log.
2. **Interactive.** In the Monitor: `uart_connect sysbus.uart0`.
3. **Host socket.** Add to `common.resc`:
   ```
   emulation CreateServerSocketTerminal 12345 "term"
   connector Connect sysbus.uart0 term
   ```
   then run `nc localhost 12345` in another terminal inside the dev container
   (to reach it from the host, forward port 12345 in VS Code's Ports panel).

You can also write UART output to a file:
`uart CreateFileBackend @uart_file` (run on the UART peripheral in the Monitor).

## 6. Debugging with GDB

Terminal 1:

```
make debug
```

This loads the ELF and starts a GDB server on port 3334. The CPU stays halted
when GDB connects so the debugger can install breakpoints before execution.

Terminal 2:

```
make gdb          # gdb-multiarch inside the container, connects to localhost:3334
```

Or use your own GDB: `arm-none-eabi-gdb target/thumbv7em-none-eabihf/debug/kernel`
then `target remote :3334`.

Typical session:

```
(gdb) break ResetHandler
(gdb) continue
(gdb) info registers
(gdb) x/8wx 0x0                # vector table
(gdb) monitor sysbus.cpu PC    # run a Renode Monitor command from GDB
(gdb) monitor sysbus ReadDoubleWord 0xE000ED04    # SCB->ICSR
```

`monitor <command>` passes any Monitor command through, so you can poke
peripherals while stopped at a breakpoint. Renode also supports GDB reverse
execution: run `reverseExecMode true` in the Monitor before connecting, then use
`reverse-step`, `reverse-stepi`, `reverse-continue` (single-core only).

## 7. Automated tests

`renode/test.robot` uses Renode's Robot Framework integration:

```
make test
```

It builds the machine, loads the ELF, attaches a terminal tester to UART0, runs
the CPU, and waits up to 5 virtual seconds for the line `kernel booted`. It
**fails until your kernel prints that line**, which makes it a handy first
milestone. Add more tests by copying the test case and changing the expected
output. Outputs (`log.html`, `report.html`, `robot_output.xml`) are written to the
repo root and are git-ignored.

Useful keywords: `Wait For Line On Uart`, `Wait For Prompt On Uart`,
`Write Line To Uart`, `Execute Command`, `Start Emulation`, `Pause Emulation`,
`Reset Emulation`. For kernel output sent through the debug console (which goes
to the Renode log, not a UART), use `Create Log Tester` and `Wait For Log Entry`
instead, as `renode/test.robot` does. Because virtual time is deterministic, these tests are not
flaky the way real-hardware tests are.

## 8. What the platform gives you (and what it does not)

From the stock `nrf52840.repl` in Renode 1.17.0:

| Peripheral | Address |
|---|---|
| Flash (1 MiB) | `0x00000000` |
| RAM (256 KiB) | `0x20000000` (also aliased at `0x00800000`) |
| NVIC | `0xE000E000` (Cortex-M4) |
| CLOCK | `0x40000000` |
| RADIO | `0x40001000` |
| UARTE0 / UARTE1 | `0x40002000` / `0x40028000` |
| TWI0 / TWI1, SPI2 | `0x40003000` / `0x40004000`, `0x40023000` |
| GPIOTE | `0x40006000` |
| TIMER0..4 | `0x40008000`, `0x40009000`, `0x4000A000`, `0x4001A000`, `0x4001B000` |
| RTC0 / RTC1 / RTC2 | `0x4000B000` / `0x40011000` / `0x40024000` |
| RNG, ECB | `0x4000D000`, `0x4000E000` |
| WDT | `0x40010000` |
| EGU0..5 | `0x40014000` .. `0x40019000` |
| PPI | `0x4001F000` |
| GPIO P0 / P1 | `0x50000500` / `0x50000800` |

Not listed means not modelled in this platform (for example SAADC, PWM, QSPI,
USBD, CRYPTOCELL). Accesses to them hit the "non-existing peripheral" warning
path. You can add your own register stubs or models in `renode/nrf52840.repl`.

**SysTick:** the platform file does not list a separate SysTick; Renode
implements it as part of its NVIC model. Verify it early by reading
`SYST_CSR` at `0xE000E010` after enabling it, rather than assuming.

## 9. Things to watch for in your bring-up

- **Vector table PC must have bit 0 set.** Renode 1.17.0 now enforces this for
  Cortex-M, as hardware does. Rust function pointers in a `static` vector table
  already have the Thumb bit set, so this only bites hand-written tables.
- **Initial SP/PC.** After `machine Reset`, check `sysbus.cpu SP` and `PC`
  against your vector table (`x/2wx 0x0` in GDB). If they are wrong, the table is
  not at address 0 or VTOR is set unexpectedly.
- **FPU.** The target is `thumbv7em-none-eabihf`. Enable CP10/CP11 in `CPACR`
  before any floating-point instruction, or you will take a UsageFault.
- **SVD file.** The stock platform applies the nRF52840 SVD (register
  definitions) from a URL. The `Dockerfile` downloads it once at image build time
  and rewrites `/opt/renode/platforms/cpus/nrf52840.repl` to use the local copy
  at `/opt/renode/svd/NRF52840.svd`, so `make run` works offline. Building the
  image needs internet, and fails if the stock platform's URL ever changes. After
  upgrading `RENODE_VERSION`, check that step still matches.
- **Virtual time.** Busy-wait delays finish as fast as the host can execute
  them. For a timer-driven kernel, test with RTC/TIMER events rather than
  relying on wall-clock sleeps.
- **Semihosting.** Renode 1.17.0's release notes list semihosting call support
  for Cortex-M. Check Renode's docs for how to enable it before relying on it;
  UART logging is the safe default.

## 10. Upgrading and running both simulators

- Upgrade Renode: change `RENODE_VERSION` in the `Dockerfile` and rebuild the dev
  container ("Dev Containers: Rebuild Container"). Release notes: https://github.com/renode/renode/releases
- `make qemu` / `make qemu-debug` run the same ELF on QEMU's generic Cortex-M4
  board (`mps2-an386`). It has none of the nRF peripherals, so it only tells you
  whether core-level behaviour (exceptions, MPU, FPU) agrees between two
  independent emulators.

## 11. Further reading

- Renode docs: https://renode.readthedocs.io
- Platform description format: https://renode.readthedocs.io/en/latest/advanced/platform_description_format.html
- GDB debugging: https://renode.readthedocs.io/en/latest/debugging/gdb.html
- UART integration: https://renode.readthedocs.io/en/latest/host-integration/uart.html
- Testing with Renode: https://renode.readthedocs.io/en/latest/introduction/testing.html