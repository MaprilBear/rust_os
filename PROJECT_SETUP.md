# Bootstrapping the Rust OS project: what was built and why

This document explains everything that was done to get this project from "an
empty folder and an idea" to "a Rust kernel that builds and boots on a simulated
nRF52840 chip inside Docker." It is written for someone who has not necessarily
used these tools before. If you already know a topic, skip that section.

It also records the wrong turns, the errors you hit, and what is and is not
verified, because those are the parts that save you time later.

---

## Table of contents

1. [The goal in one paragraph](#1-the-goal-in-one-paragraph)
2. [The big picture](#2-the-big-picture)
3. [Concepts you need (a beginner's glossary)](#3-concepts-you-need-a-beginners-glossary)
4. [How we got here: decisions and detours, in order](#4-how-we-got-here-decisions-and-detours-in-order)
5. [File-by-file reference](#5-file-by-file-reference)
6. [Command reference](#6-command-reference)
7. [What has been verified, and what has not](#7-what-has-been-verified-and-what-has-not)
8. [Troubleshooting](#8-troubleshooting)
9. [What to do next](#9-what-to-do-next)
10. [Sources](#10-sources)

---

## 1. The goal in one paragraph

You want to write an operating system in Rust for an ARM **Cortex-M4**
microcontroller, and eventually run it on a real **Nordic nRF52840** chip. Until
you have hardware (or whenever you do not want to flash it), you want to run and
debug the OS on your PC using **system-level emulation**, inside **Docker** so
the setup is reproducible and does not pollute your Arch Linux install. You
already know how to write an OS, so the scope here was the *environment only*:
toolchain, emulator, build and run commands, debugging, and testing. Writing the
low-level startup code (the "bring-up") is deliberately left to you.

---

## 2. The big picture

```
 Your Arch Linux PC
 ┌──────────────────────────────────────────────────────────────────┐
 │  Your editor + git repo (this folder)                            │
 │      │                                                           │
 │      │  `make build`, `make run`, ...                            │
 │      ▼                                                           │
 │  Docker container (image: nrf52840-os-dev)                       │
 │  ┌────────────────────────────────────────────────────────────┐  │
 │  │  Rust toolchain ──compiles──► ELF file (your kernel)       │  │
 │  │                                   │                        │  │
 │  │                                   ▼                        │  │
 │  │  Renode: a simulated nRF52840 chip                         │  │
 │  │     Cortex-M4 CPU + flash + RAM + UART + timers + ...      │  │
 │  │     (or QEMU: a simulated generic Cortex-M4 board)         │  │
 │  │                                   ▲                        │  │
 │  │  gdb-multiarch ───debug───────────┘                        │  │
 │  └────────────────────────────────────────────────────────────┘  │
 └──────────────────────────────────────────────────────────────────┘
```

Your repo folder is mounted into the container at `/work`, so files you edit on
the host are immediately visible inside, and build output (`target/`) lands back
in your folder owned by your user, not by root.

---

## 3. Concepts you need (a beginner's glossary)

### Cortex-M4 and the nRF52840

A **microcontroller** is a tiny computer on one chip: a CPU, a bit of memory, and
peripherals (serial ports, timers, GPIO pins) all together. The **Cortex-M4** is
a CPU design from Arm, common in microcontrollers. It runs the **ARMv7E-M**
instruction set in **Thumb** mode, has an optional hardware floating-point unit
(FPU), and has built-in features an OS cares about: an interrupt controller
(**NVIC**), a system timer (**SysTick**), and an optional memory protection unit
(**MPU**).

The **nRF52840** is Nordic Semiconductor's chip that contains a Cortex-M4F core
plus 1 MiB of flash, 256 KiB of RAM, Bluetooth radio, USB, and many peripherals.
It is the real chip you plan to target.

### Bare metal, `no_std`, and `no_main`

"Bare metal" means your code runs directly on the hardware with no operating
system underneath (you *are* the OS). In Rust that means:

- `#![no_std]`: you cannot use Rust's standard library (it assumes an OS for
  files, threads, heap allocation, and so on). You get `core`, which is the
  OS-independent subset.
- `#![no_main]`: Rust's usual `fn main()` startup is not used. On a Cortex-M,
  the hardware itself decides where execution starts (see "vector table").
- You must provide your own `#[panic_handler]`, because the standard one is
  part of `std`.

### The target triple: `thumbv7em-none-eabihf`

This string tells the Rust compiler what to generate code for:

| Part | Meaning |
|---|---|
| `thumb` | Thumb instruction encoding (what Cortex-M uses; there is no classic Arm mode) |
| `v7em` | ARMv7E-M architecture, which is the Cortex-M4 (and M7) |
| `none` | no operating system |
| `eabi` | the Arm embedded ABI (calling convention) |
| `hf` | "hard float": floating-point values are passed in FPU registers |

Because we chose `hf`, the FPU **must be switched on** early in startup (by
setting CP10/CP11 access bits in the `CPACR` register). If code executes a
floating-point instruction before that, the CPU raises a fault. If you would
rather avoid that for now, `thumbv7em-none-eabi` (no `hf`) uses software float.

### ELF files

When Rust links your kernel it produces an **ELF** file: your machine code and
data plus metadata (where each piece goes in memory, symbol names, debug info).
Simulators and debuggers load the ELF directly. It is the same kind of file you
would eventually flash to real hardware (usually converted to `.hex` or `.bin`).

### Linker script (`link.ld`)

The linker decides *where in memory* each part of your program lives. On a PC the
OS decides that for you; on bare metal you tell the linker explicitly with a
**linker script**: "flash is here and this big, RAM is here and this big, put
code in flash, put variables in RAM," and so on. It can also define symbols
(like `_estack`, the top of RAM) that your code can refer to.

### The vector table

This is the single most important structure on a Cortex-M. It is an array of
32-bit values at the start of memory (address `0x00000000` on the nRF52840).
When the chip powers on or resets, the CPU hardware reads:

- word 0: the **initial stack pointer** (it loads this into `SP`),
- word 1: the **reset vector** (the address of your first instruction, loaded
  into `PC`).

Every later entry is the address of a handler for an exception or interrupt
(HardFault, SysTick, UART interrupt, and so on). One subtle rule: handler
addresses must have **bit 0 set** (the "Thumb bit") to say "this is Thumb code."
Rust function pointers already carry this bit, which is why a Rust `static`
vector table works without special handling.

### Emulator vs simulator, QEMU vs Renode

Both run your unmodified binary on a *virtual* chip, but they have different
philosophies:

- **QEMU** is a very fast CPU emulator with a fixed list of ready-made machines.
  It has no nRF52 machine.
- **Renode** (from Antmicro) simulates a *whole machine* described in a text
  file: CPU, memory, and each peripheral at its address. It has an nRF52840
  platform with working UARTE, timers, RTCs, GPIO, and more.

More on why we chose Renode in section 4.

### UART and UARTE

A **UART** is a simple serial port, the classic way to print debug text from a
microcontroller. The nRF52840's version is called **UARTE** (the "E" is for
EasyDMA): you give it a pointer to a buffer in RAM and a length, start it, and it
sends the bytes by itself without the CPU copying each one. UARTE0 lives at
address `0x40002000`. Your kernel printing text to UARTE0 is how you will see
output in the simulator.

### SVD

An **SVD** file is an XML description of a chip's registers (names, addresses,
bit fields). Renode loads the nRF52840's SVD so it can log accesses by register
name, which makes debugging much more readable.

### GDB

**GDB** is the standard debugger. Both Renode and QEMU can expose a "GDB server"
on a network port; GDB connects to it and lets you set breakpoints, step
instructions, and inspect registers and memory, as if the simulated chip were
real hardware on a debug probe.

### Docker, in two sentences

A Docker **image** is a frozen filesystem with all the tools installed (like a
tiny, disposable Linux install). A **container** is a running instance of an
image; ours are started with `--rm` so they vanish when the command ends, and
your project folder is mounted in so nothing you care about lives inside.

---

## 4. How we got here: decisions and detours, in order

### Step 1: First attempt, QEMU only

The first version of the environment used **QEMU** because that is what you
asked for. QEMU has no generic "Cortex-M4 board," so the closest option is
`-machine mps2-an386 -cpu cortex-m4`. The `mps2-an386` machine models an Arm
development board (MPS2) loaded with a Cortex-M4 design. It provides a real
Cortex-M4 CPU plus memory: 4 MiB at `0x00000000` and 4 MiB of RAM at
`0x20000000`. Semihosting (a way for the program to print through the emulator
using special instructions) was enabled for early output.

This version had a Dockerfile, a Makefile, a Cargo project skeleton, a linker
script, and a stub `main.rs`.

### Step 2: "Use micro:bit instead", and why that was not done

You asked to switch to QEMU's `microbit` machine because you plan to run on an
nRF52840. We checked QEMU's documentation first, and it turned out the QEMU
`microbit` machine is the **original micro:bit (v1) with an nRF51822 chip**,
which has a **Cortex-M0**, a smaller, older core (ARMv6-M). QEMU's own docs also
state that the nRF52, nRF53 and nRF91 series are **not supported**.

Using it would have meant developing against the wrong CPU: different Rust
target (`thumbv6m-none-eabi`), no FPU, a smaller instruction set, and missing
features an OS cares about. So instead of switching, the Cortex-M4 setup was
kept, and `link.ld` was changed to match the nRF52840's memory map:

| Region | Start | Size |
|---|---|---|
| Flash | `0x00000000` | 1 MiB |
| RAM | `0x20000000` | 256 KiB |

QEMU's `mps2-an386` maps *more* memory than that at the same addresses, so the
same kernel still runs there.

The remaining gap was peripherals: QEMU has none of the nRF52840's peripherals
(UARTE, GPIOTE, RTC, TIMER, radio). That led to the next step.

### Step 3: QEMU vs Renode comparison

You asked for a comparison because you had not used Renode. The summary:

| | QEMU | Renode |
|---|---|---|
| nRF52840 | No | Yes (UARTE, GPIO, timers, RTC, SPI/I2C, partial radio) |
| Cortex-M4 core testing | Yes, via `mps2-an386` | Yes, via the nRF52840 platform |
| Speed | Faster | Slower, but fine for kernel work |
| Debugging | `-S -gdb tcp::1234` | Built-in GDB server plus a Monitor shell to inspect and poke registers and peripherals |
| Testing | Manual scripts | Robot Framework integration (`renode-test`) for CI |
| Adding hardware | Write C inside QEMU | Write a `.repl` text file |

The caveat on Renode: its peripheral models are written from the reference
manual by Antmicro, so a driver that works in Renode is "works in the model,"
not "works on silicon." Treat the real board as the final judge.

### Step 4: Rework to Renode

You chose Renode. We looked up Renode's current release (**1.17.0**), its GDB
command, UART options, and the contents of its stock nRF52840 platform file, and
reworked the setup:

- The Dockerfile now installs the **portable Linux build** of Renode (it bundles
  its own .NET runtime, so no .NET install is needed) plus Python's Robot
  Framework for tests. QEMU stays installed as an optional second opinion.
- A `renode/` folder was added with the platform file, scripts, a cargo runner,
  and a test.
- The Makefile gained `run`, `debug`, `gdb` and `test` targets for Renode, with
  `qemu` and `qemu-debug` kept for the core-only check.
- `.cargo/config.toml` now makes `cargo run` boot in Renode.
- `RENODE_TUTORIAL.md` was written to teach Renode as it applies to this
  project.

The stock Renode nRF52840 platform matches your linker script exactly (flash 1
MiB at `0x0`, RAM 256 KiB at `0x20000000`).

### Step 5: Docker was not running

Your first `make image` failed with:

```
failed to connect to the docker API at unix:///var/run/docker.sock
```

Nothing was wrong with the project. The Docker daemon simply was not running, so
the socket file did not exist. The fix on Arch is:

```
sudo systemctl enable --now docker.service
```

`enable` makes it start at boot; `--now` starts it immediately. (If you get a
*permission denied* error afterwards, add yourself to the `docker` group with
`sudo usermod -aG docker $USER` and log out and back in.) The warning
`DEPRECATED: The legacy builder` is harmless and is silenced by installing
`docker-buildx`.

### Step 6: The `-nostartfiles` linker error (a mistake in the original files)

`make build` then failed:

```
rust-lld: error: unknown argument '-nostartfiles'
```

This was an error in the original `.cargo/config.toml`. `-nostartfiles` is a flag
for the **gcc** linker driver. For this target Rust calls the linker
**`rust-lld` directly**, which does not understand it. It was also unnecessary,
since a freestanding Rust program with `#![no_main]` does not link C startup
files anyway. The flag was removed, leaving only `-Tlink.ld` (use our linker
script).

### Step 7: The first run, and how to read its output

After the fix, `make run` worked end to end: the image built, Rust compiled and
linked, Renode started, loaded the ELF, and started the machine. The output
looked alarming but was correct for a project with no vector table yet:

```
cpu: Setting initial values: PC = 0xE7FE, SP = 0x466FB580.
sysbus: ... WriteDoubleWord to non existing peripheral at 0x466FB57C ...
(nrf52840) sysbus.cpu PC
0xeffffffe
```

Why those numbers? The stub kernel had no vector table, so the linker placed the
stub's machine code at address 0. Renode treats the first two 32-bit words at
address 0 as the initial `SP` and `PC`. Those words were really instruction
bytes:

- `SP = 0x466FB580`: the bytes of the instructions `push {r7, lr}` (`0xB580`)
  and `mov r7, sp` (`0x466F`), read as one number.
- `PC = 0xE7FE`: the machine code for `b .` (branch to self, an infinite loop),
  read as an address. Its Thumb bit is also clear, which Renode 1.17.0 now flags
  just like real hardware would.

The CPU jumped to garbage and faulted. The repeated "WriteDoubleWord to non
existing peripheral" warnings were the CPU trying to push exception state onto
the garbage stack pointer, and `0xeffffffe` is the special value a Cortex-M puts
in `PC` while it is in a lock-up or exception-return state. In short, the
toolchain, Docker, Renode, and ELF loading were all working. Only the vector
table was missing, which is the next step.

### Step 8: The vector table

`src/vectors.rs` was added and `src/main.rs` updated to include it (`mod
vectors;`). What it contains:

- **Word 0:** initial stack pointer, `_estack` (the top of RAM, `0x20040000`,
  defined in `link.ld`).
- **Word 1:** the address of `Reset`.
- **14 system exception slots** (NMI, HardFault, MemManage, BusFault,
  UsageFault, SVCall, DebugMonitor, PendSV, SysTick, plus reserved slots set to
  zero), laid out in the exact order the hardware requires.
- **48 external interrupt slots**, which is the nRF52840's number of IRQ lines.
- Every real handler points at one shared `default_handler` that loops forever,
  so an unexpected exception is easy to spot (pause the simulator and look at
  `PC`). You replace entries with your own handlers as you write drivers.

The table is a Rust `static` marked `#[link_section = ".vector_table"]`,
`#[no_mangle]` and `#[used]`, and `link.ld` places that section first, at address
0, and uses `KEEP` so the linker never discards it. It uses a Rust `union` for each
slot so a slot can hold either a function pointer or a plain zero.

This was type-checked (compiled against the host machine) but could not be built
for the ARM target in the environment where it was written; see section 7.

### Step 9: Caching the SVD file

Every `make run` printed:

```
Downloading https://dl.antmicro.com/projects/renode/svd/NRF52840.svd.gz.
```

That is Renode's stock nRF52840 platform fetching the register definitions each
time, because every container starts with an empty `/tmp`. (An earlier version of
`RENODE_TUTORIAL.md` guessed that Renode kept the file under `$HOME` where a
persistent mount would cache it. Your log showed it actually goes to a
randomly named temp file, so a mounted cache would not have been reused. That
guess was wrong and the tutorial text was corrected.)

The fix: bake the file into the Docker image. A new Dockerfile step downloads the
SVD once at image build time, unzips it to `/opt/renode/svd/NRF52840.svd`, and
rewrites the URL inside Renode's stock `nrf52840.repl` to point at that local
file. A `grep` check at the end of the step makes the build fail loudly if the
URL line in the stock platform ever changes, instead of silently falling back to
downloading. After you rebuild the image, runs work offline.

### Step 10: A dev container for rust-analyzer

**The problem.** You asked for better rust-analyzer results. rust-analyzer is
your editor's "language server": it provides completions, go-to-definition,
inline errors, and so on, by actually running the Rust compiler's analysis on
your code. For this project it needs three things to be right:

1. the **`thumbv7em-none-eabihf` target**, because the code is `no_std` and
   cannot be analyzed as ordinary PC code,
2. the **standard library source** (`rust-src`), so it can show you `core`,
3. **the same compiler version** you build with.

All of those live inside the Docker image, not on your Arch install. If your
editor runs on the host, rust-analyzer sees a different (or missing) toolchain
and reports confusing errors.

**The fix.** `.devcontainer/devcontainer.json` tells VS Code to open the project
*inside* the image from the repo's `Dockerfile`, so rust-analyzer runs next to
the real toolchain and sees exactly what `cargo build` sees. Three supporting
changes were made:

- **`rust-analyzer` was added to the image.** It was missing: the Dockerfile
  installed `rust-src`, `clippy` and `rustfmt` but not the `rust-analyzer`
  component. It is now installed with `rustup component add`, and also listed in
  `rust-toolchain.toml`. Using the toolchain's own copy means it always matches
  the compiler (the editor extension would otherwise download its own binary).
- **A non-root `dev` user (UID 1000) was added to the image.** Dev containers
  usually run as a normal user. VS Code changes that user's UID to match your
  host user, so files created from the editor (including `target/`) are owned by
  you, as with the Makefile's `--user`.
- **Settings tuned for a `no_std` kernel** (see the table in the file reference),
  chiefly: analyze for the ARM target, and skip test and bench targets, which
  cannot build here and would otherwise show false errors.

**How to use it.**

1. Nothing to build by hand: opening the dev container builds the image from
   the `Dockerfile` (cached afterwards). If you changed the `Dockerfile`, use the
   command **"Dev Containers: Rebuild Container"**.
2. In VS Code, install the **Dev Containers** extension, open this folder, and
   run the command **"Dev Containers: Reopen in Container"**. The first open
   builds the image (cached afterwards) and installs the extensions in the
   container.
3. Open `src/main.rs`. rust-analyzer starts and indexes. Hover and
   go-to-definition into `core` should work.

**What changes in your workflow.** Docker is not available *inside* the dev
container, and the original Makefile worked by calling `docker run`. So the
Makefile was changed (Step 11) to assume it is already running inside the
container: open a terminal in VS Code and use `make build`, `make run`, and so
on directly. GDB and Renode run in the same container, so `localhost:3334` just
works and no `--network host` is needed.

**Not using VS Code?** The `devcontainer.json` is a standard format. The
`devcontainer` CLI can build and start the same container for other editors, and
you can point an editor's language-server setting at the container's
`rust-analyzer` (for example by running it through `docker exec`). The extension
list and settings in the file are VS Code specific, so copy the `rust-analyzer.*`
settings into your editor's equivalent.

### Step 11: A Makefile for inside the container, and printing to the console

**Makefile.** The original Makefile wrapped every command in `docker run`,
because the plan was to work from your host. Once the dev container existed, that
no longer made sense: Docker is not available inside it, and you are already in
the right environment. So the `docker run` prefix and the `image` and `shell`
targets were removed, and every target is now the plain command (`make build` is
`cargo build`, `make run` is `cargo build` then Renode, and so on). The
container-only `-it`, `--user`, `-v` and `--network host` flags are no longer
needed. A comment at the top of the file keeps the one-line `docker run`
equivalent in case you ever want to run a target from the host.

**Printing from the kernel.** You asked how to print from the OS to the Renode
console. There is no special "console" call on a microcontroller: you write
bytes to a serial port (the UART), and the simulator shows what comes out of it.
The pieces:

1. **The kernel writes to UARTE0.** `src/uart.rs` configures the nRF52840's UARTE0
   peripheral (at address `0x40002000`, the same address as on the real chip),
   then sends text through it. The UARTE sends data using **EasyDMA**: you write
   a pointer to a buffer and a length into its registers, trigger the "start
   transmit" task, and the hardware reads the bytes out of RAM itself. The code
   then waits for the "end of transmit" event before sending the next chunk.
2. **The buffer must be in RAM.** String literals like `"kernel booted"` are
   stored in flash, and EasyDMA is only allowed to read from RAM. So the code
   copies the text into a 64-byte buffer on the stack and sends it in chunks of
   up to 64 bytes.
3. **`print!` and `println!` are provided.** `uart.rs` implements Rust's
   `core::fmt::Write` for a small `Uart` type, so formatting works the way you
   expect: `println!("tick {}", n)`. `main.rs` loads them with `#[macro_use] mod
   uart;`. There is no heap involved.
4. **Renode shows the UART output.** `renode/common.resc` contains `showAnalyzer
   sysbus.uart0`. When Renode runs without a GUI (as `make run` does), a UART
   analyzer prints what the UART transmits into Renode's log in the terminal.
5. **Panics print too.** The panic handler now prints `PANIC: <message and
   location>` before looping, so a failed `unwrap` or array index is visible
   instead of a silent hang.

`Reset` now calls `uart::init()` and prints `kernel booted`. That is exactly the
line `renode/test.robot` waits for, so `make test` should now pass. It needs no
`.data` or `.bss` setup, because the UART code uses only the stack and fixed
hardware addresses, so it works even before you write your real bring-up.

Things to know:

- **Pins.** `init()` selects pin P0.06 for TX (the pin wired to the USB serial
  port on the nRF52840 development kit) and leaves the other pins disconnected.
  Renode does not care which pin you pick, but real hardware does: change
  `TX_PIN` in `uart.rs` for your board.
- **It blocks.** `write_bytes` spins until the hardware reports the chunk sent.
  If the UART is not enabled or the address is wrong, the kernel hangs at the
  first print. If that happens, pause Renode and check `sysbus.cpu PC`: it will
  be inside the `while read_reg(EVENTS_ENDTX) == 0` loop.
- **Interactive view.** For a raw, character-by-character view instead of log
  lines, type `uart_connect sysbus.uart0` at the Renode prompt (press `Esc` to
  go back). The tutorial also shows how to expose the UART on a TCP port.
- **Semihosting is not used.** Semihosting is another way to print, but it is
  QEMU-oriented, and it is not clear how to enable it in Renode, so the UART path
  is the dependable one.

> **Update:** Step 12 switched the kernel's default printing from this UART code
> to a simpler debug console. `src/uart.rs` is still in the repo for when you
> write your real UART driver, but `main.rs` no longer includes it. The UART
> notes above still describe how that code works.

### Step 12: A quick-and-dirty debug console (no UART)

**The request.** You plan to write your own UART driver later and wanted a way
to print for debugging without using any of the chip's peripherals, by calling
out to the emulator directly.

**What was looked into.**

- **Semihosting** is the standard "call out to the host" mechanism: the program
  executes a special instruction and the debugger or emulator prints for it. It
  is already enabled for QEMU in `make qemu`. For Renode it is not a dependable
  choice here: the Renode 1.17.0 release notes say semihosting calls such as
  `SYS_WRITE` were added for Arm, but no documentation was found for how to turn
  it on for a Cortex-M machine, and the only lead is an unanswered community
  question about a `UART.SemihostingUart` peripheral. It would also need a
  special instruction in your code, which touches the assembly-level work you
  said to leave alone for now.
- **A custom peripheral in Renode** is documented and fully under your control.
  Renode lets you declare a peripheral whose behaviour is a short Python script.
  So we made up a one-register device that does not exist on the real chip.

**What was built.**

1. **`renode/debugcon.py`** is the script. Whenever the CPU writes a value to the
   peripheral, it takes the low byte, adds it to a line buffer, and when it sees a
   newline (`\n`) prints the line to the Renode log as `debugcon: <your text>`.
2. **`renode/nrf52840.repl`** now declares the peripheral at address
   `0x60000000` (a 4-byte `Python.PythonPeripheral` using that script). That
   address is unused on the nRF52840 in this platform.
3. **`src/debugcon.rs`** is the kernel side: a function that writes a byte to
   `0x60000000`, and two macros, `dprint!` and `dprintln!`, that work like
   `print!` and `println!`: `dprintln!("tick {}", n)`. It needs no setup, so it
   works from the first instruction of `Reset`, before `.data`, `.bss`, the clock
   or any pins are configured.
4. **`src/main.rs`** now prints `kernel booted` with `dprintln!`, and the panic
   handler prints `PANIC: ...` the same way.
5. **`renode/test.robot`** now waits for the log line `debugcon: kernel booted`
   using Renode's log tester (`Create Log Tester`, `Wait For Log Entry`) instead
   of a UART terminal tester.

**Keeping it off the real chip.** On a real nRF52840 nothing exists at
`0x60000000`, so writing there would cause a bus fault. The macros are therefore
controlled by a Cargo feature named `debugcon`, which is on by default
(`Cargo.toml` has `default = ["debugcon"]`). For hardware, build with
`cargo build --no-default-features`; the macros then compile to nothing. (The
check is `if cfg!(feature = "debugcon")`, so the arguments are still type-checked
either way.)

**Things to know.**

- **Output appears per line.** Nothing shows until a `\n` is sent, so use
  `dprintln!`, and if you use `dprint!` finish the line yourself.
- **Braces are safe.** The script logs with `self.InfoLog("{0}", line)` rather than
  putting your text in the format string, so output like `Foo { a: 1 }` cannot
  break Renode's log formatting.
- **It is slow-ish.** Each byte is a separate simulated bus write that runs a
  Python script. That is fine for debug prints but not for printing megabytes.
- **The script path.** The `.repl` refers to the script as `renode/debugcon.py`,
  relative to where Renode is started (the repo root, which is what the Makefile
  does). If Renode says it cannot find the file, change it to an absolute path
  such as `/work/renode/debugcon.py`.
- **Use your own UART later.** When your driver exists, you can keep both: the
  debug console stays handy for early boot, before the UART is configured.

---

## 5. File-by-file reference

```
rust_os/
├── Dockerfile
├── Makefile
├── Cargo.toml
├── rust-toolchain.toml
├── link.ld
├── .gitignore
├── RENODE_TUTORIAL.md
├── PROJECT_SETUP.md          (this file)
├── .devcontainer/
│   └── devcontainer.json
├── .cargo/
│   └── config.toml
├── src/
│   ├── main.rs
│   ├── debugcon.rs
│   ├── uart.rs           (kept for reference, not compiled in)
│   └── vectors.rs
└── renode/
    ├── debugcon.py
    ├── nrf52840.repl
    ├── common.resc
    ├── run.resc
    ├── debug.resc
    ├── run.sh
    └── test.robot
```

### `Dockerfile`

Builds the development image, `nrf52840-os-dev`. It starts from `rust:1-trixie`
(the official Rust image on Debian 13 "trixie") and layers on:

1. **System packages:** `qemu-system-arm` (optional second emulator),
   `gdb-multiarch` (a GDB that understands ARM), `binutils-arm-none-eabi`
   (inspection tools), `make`, `git`, `wget`, `ca-certificates`, and Python 3
   with pip and venv.
2. **Renode 1.17.0:** downloaded from GitHub releases as the `linux-portable`
   tarball into `/opt/renode`, with `renode` and `renode-test` symlinked onto the
   `PATH`. `RENODE_VERSION` is a build argument at the top of that step, so
   upgrading means changing one number and rebuilding.
3. **SVD file:** downloaded and baked in (Step 9 above).
4. **Robot Framework:** installed into a virtual environment at `/opt/venv`
   (Debian forbids installing Python packages globally), using Renode's own
   requirements file if present. This is what `renode-test` uses.
5. **`DOTNET_SYSTEM_GLOBALIZATION_INVARIANT=1`:** tells Renode's bundled .NET
   runtime to skip needing the ICU internationalization libraries.
6. **Rust toolchain:** the `thumbv7em-none-eabihf` target, plus `rust-src`,
   `llvm-tools`, `clippy` (linter), `rustfmt` (formatter), `rust-analyzer`
   (the language server your editor uses; added with the dev container), and
   `cargo-binutils` (so `cargo size` and friends work).
7. **`CARGO_HOME=/cargo-home`, world-writable:** the container runs as *your*
   user id (not root) so build output is not root-owned on your disk. Cargo needs
   a writable home directory for its downloaded crates cache, so this is a
   directory anyone can write to. The dev container backs it with a persistent
   Docker volume (`nrf52840-cargo-home`).
8. **A `dev` user (UID 1000):** the user the dev container runs as. VS Code
   remaps its UID to match yours.

### `Makefile`

The Makefile assumes it is running **inside the container** (the dev container),
where `cargo`, `renode`, `renode-test`, `qemu-system-arm` and `gdb-multiarch` are
all on the `PATH`. Each target is just the plain command, for example `make run`
runs `cargo build` and then `renode --disable-gui --console renode/run.resc`.
Targets are listed in section 6.

It sets three variables: `ELF` (the path to the built kernel, derived from the
target triple and the `PROFILE` variable, `debug` by default), and `GDB_PORT`
(3334, used by `make gdb`). The first lines of the file contain a `docker run`
one-liner for running any target from the host if you ever need to; it is only a
comment, not part of the build.

### `Cargo.toml`

Names the package `kernel` (so the binary and ELF are called `kernel`, matching
the Makefile's `ELF` path). Both profiles set `panic = "abort"` because bare
metal has no stack unwinding. The dev profile uses `opt-level = 1`, since
unoptimized (level 0) Rust is very large and slow for embedded targets, while
level 1 stays reasonably debuggable. The release profile optimizes for size
(`"s"`), uses link-time optimization, and one codegen unit; both keep debug info
(`debug = true`) so GDB can show source lines. The cargo output
"`dev` profile [optimized + debuginfo]" is just a description of these settings.

### `rust-toolchain.toml`

Pins the toolchain channel to `stable` and lists the target and components
(including `rust-analyzer`), so anyone running plain `cargo` inside the project
gets the same setup. Keep it in sync with the Dockerfile's `rustup` step.

### `.cargo/config.toml`

Cargo configuration for this project:

- `[build] target`: always cross-compile for `thumbv7em-none-eabihf`, so you can
  type plain `cargo build`.
- `runner = "sh renode/run.sh"`: makes `cargo run` boot the kernel in Renode.
- `rustflags = ["-C", "link-arg=-Tlink.ld"]`: pass `-Tlink.ld` to the linker so
  it uses your linker script.

### `link.ld`

The linker script. It declares:

- **Memory:** `FLASH` at `0x00000000`, 1 MiB; `RAM` at `0x20000000`, 256 KiB
  (the nRF52840's real sizes).
- **`ENTRY(Reset)`:** the program's entry symbol (mostly informational on a
  Cortex-M, since the vector table is what the hardware actually reads).
- **`.vector_table`:** placed first, at the start of flash, with `KEEP` so it
  can never be garbage-collected.
- **`.text`, `.rodata`:** code and read-only data in flash.
- **`.data`:** initialized variables. They *live* in flash but must be *copied*
  to RAM at startup (`> RAM AT > FLASH`). The script exports `_sdata`, `_edata`
  (RAM range) and `_sidata` (where the initial values are stored in flash) for
  your startup code to do the copy.
- **`.bss`:** zero-initialized variables, in RAM, not stored in the file. It
  exports `_sbss` and `_ebss` so your startup code can zero the range.
- **`_estack`:** the top of RAM, used as the initial stack pointer.
- **`/DISCARD/`:** drops Arm exception-unwinding tables that bare metal does not
  use.

### `src/main.rs`

A minimal `#![no_std]`, `#![no_main]` program. It includes `mod debugcon;` (with
`#[macro_use]`, so `dprint!`/`dprintln!` work) and `mod vectors;`. It defines a
placeholder `Reset` function that prints `kernel booted` and loops (replace it
with real bring-up), and a `#[panic_handler]` that prints `PANIC: ...` and then
loops.

### `src/debugcon.rs`

The kernel side of the debug console (Step 12): `putc()` writes one byte to the
made-up peripheral at `0x60000000`, a `DebugCon` type implements
`core::fmt::Write`, and the `dprint!`/`dprintln!` macros use it. Gated by the
`debugcon` Cargo feature so it can be compiled out for real hardware.

### `src/uart.rs`

**Not compiled in since Step 12** (`main.rs` has no `mod uart;`); kept as a
starting point for your own UART driver. A small blocking transmit driver for
UARTE0 plus the `print!` and `println!` macros (Step 11). `init()` sets the pins, baud rate (115200) and enables the
peripheral; `write_bytes()` sends data through EasyDMA in 64-byte chunks from a
RAM buffer; `Uart` implements `core::fmt::Write`. The register offsets are at the
top of the file with the datasheet names.

### `src/vectors.rs`

The vector table (Step 8). Replace entries in `exceptions` and `irqs` with your
own handlers as you write them.

### `.devcontainer/devcontainer.json`

Tells VS Code (and other tools that support the dev container spec) how to open
this project *inside* the Docker image, so rust-analyzer runs next to the real
toolchain. See Step 10 for why that matters. Its settings:

| Setting | What it does |
|---|---|
| `build.dockerfile` / `context` | Build the container from the repo's own `Dockerfile`, so the dev container and the Makefile use the same tools. |
| `remoteUser: dev` + `updateRemoteUserUID` | Run as the non-root `dev` user, with its UID changed to match yours so files are not root-owned. |
| `workspaceMount` / `workspaceFolder` | Mount your repo at `/work`, the same path the Makefile uses. |
| `mounts` | Reuse the `nrf52840-cargo-home` Docker volume, so the Makefile and the dev container share one cargo cache. |
| `extensions` | Installs `rust-lang.rust-analyzer` and `tamasfe.even-better-toml` in the container. |
| `rust-analyzer.cargo.target` | Analyze for `thumbv7em-none-eabihf`, not your PC. |
| `rust-analyzer.cargo.allTargets` / `check.allTargets` = `false` | Skip test and bench targets. A `no_std` kernel has no test harness (it needs `std`), so analyzing them produces false errors like "can't find crate for `test`". |
| `rust-analyzer.check.command = clippy` | Run clippy instead of plain `cargo check` on save. |
| `files.watcherExclude` | Stops the editor watching `target/`, which is large and noisy. |
| `[rust]` formatter | Format on save with rustfmt (via rust-analyzer). |

### `.gitignore`

Ignores `target/` (build output) and the Robot Framework output files that tests
write into the repo root (`log.html`, `report.html`, `robot_output.xml`).

### `renode/nrf52840.repl`

The *platform description*: it just says `using
"platforms/cpus/nrf52840.repl"`, which pulls in Renode's stock nRF52840 (Cortex-M4
with NVIC, 1 MiB flash at `0x0`, 256 KiB RAM at `0x20000000`, UARTE0/1, RTC0-2,
TIMER0-4, GPIO P0/P1, GPIOTE, TWI, SPI, PPI, EGU0-5, RNG, ECB, radio, I2S, PDM,
WDT, CLOCK). It exists as its own file so you have a place to add board-level
devices (LEDs, buttons, extra stubs) without touching Renode's installation.
Since Step 12 it also declares the `debugcon` Python peripheral at `0x60000000`.

### `renode/debugcon.py`

The behaviour of the debug-console peripheral (Step 12): collects written bytes
into a line and prints the line to the Renode log as `debugcon: <line>` when a
newline arrives.

### `renode/common.resc`

A Renode *script*, shared by `run.resc` and `debug.resc`. It:

1. sets `$bin` (the ELF path) with a default, which `run.sh` can override,
2. creates a machine called `nrf52840`,
3. loads the platform description,
4. shows UART0's output in the log (`showAnalyzer sysbus.uart0`),
5. loads your ELF onto the machine's bus (`sysbus LoadELF $bin`).

### `renode/run.resc`

Includes `common.resc`, then `start`s the simulation.

### `renode/debug.resc`

Includes `common.resc`, then starts a GDB server on port 3334 with
`machine StartGdbServer 3334 false`. The CPU stays halted when GDB connects so
the debugger can install breakpoints before execution.

### `renode/run.sh`

The cargo runner. When you run `cargo run`, cargo calls it with the ELF path; it
launches Renode headless with `$bin` set to that ELF and includes `run.resc`.

### `renode/test.robot`

An automated test using Renode's Robot Framework integration. It creates the
machine, loads the ELF, creates a log tester, starts the CPU, and waits for the
Renode log line `debugcon: kernel booted`, which the kernel's `dprintln!`
produces (Step 12). It was written to fail until the kernel prints that line;
the kernel now does, so it should pass. Keep it as a regression test and add
more.

### `RENODE_TUTORIAL.md`

A project-specific Renode tutorial: mental model, Monitor commands, UART output
options, GDB sessions, writing tests, the peripheral map, and bring-up gotchas.

---

## 6. Command reference

Run all of these from the repo root, in a terminal **inside the dev container**.
(There is no `make image` or `make shell` any more; the dev container builds the
image for you and its terminal is the shell.)

| Command | What it does |
|---|---|
| `make build` | `cargo build` (debug). |
| `make release` | `cargo build --release`. |
| `make run` | Build, then boot the kernel on the simulated nRF52840 in Renode. You get a `(nrf52840)` prompt. Type `quit` to exit. |
| `make debug` | Build, then start Renode with a GDB server on port 3334. |
| `make gdb` | In a second terminal: attach `gdb-multiarch` to that server. |
| `make test` | Run `renode/test.robot` via `renode-test`. |
| `make qemu` | Run the same ELF on QEMU's generic Cortex-M4 board (no nRF peripherals). Exit with `Ctrl-A` then `X`. |
| `make qemu-debug` | QEMU halted with a GDB server on port 1234. |
| `make clippy` / `make fmt` | Lint / format the code. |
| `make size` | Show section sizes of the built ELF. |
| `make clean` | Remove build output and test reports. |

Handy Renode Monitor commands at the `(nrf52840)` prompt:

```
pause                        stop the simulation
start                        resume it
sysbus.cpu PC                show the program counter
sysbus.cpu SP                show the stack pointer
machine Reset                reset the machine
peripherals                  list every simulated peripheral
sysbus ReadDoubleWord 0x0    read a 32-bit value (e.g. vector table word 0)
quit                         leave Renode
```

`RENODE_TUTORIAL.md` has the full cheat sheet.

---

## 7. What has been verified, and what has not

It is worth being exact about this.

**Confirmed working** (you ran these and pasted the output):

- `make image` builds the Docker image (after starting the Docker daemon).
- `make build` compiles and links the original stub kernel (after removing
  `-nostartfiles`).
- `make run` starts Renode 1.17.0, downloads the SVD, loads the ELF, and starts
  the simulated machine. The garbage `PC`/`SP` values you saw were explained by
  the missing vector table.

**Written and checked only as far as noted:**

- **The vector table (`src/vectors.rs`).** It was type-checked by compiling for
  the host PC, with no warnings, but it was **not** built for the Cortex-M4
  target or run in Renode, because the environment it was written in could not
  download the ARM target. The first real test is yours: run `make build`, then
  `make run`, `pause`, and check that `sysbus.cpu SP` shows `0x20040000` and
  `sysbus.cpu PC` is inside `Reset` (not `default_handler`). `sysbus
  ReadDoubleWord 0x0` and `0x4` read the first two table entries directly if
  something looks off.
- **The baked-in SVD.** The Dockerfile step has not been built. If it fails,
  the likely causes are a changed URL in Renode's stock platform (the `grep`
  guard exists to catch exactly that) or no internet while the image builds.
- **`make test`, `make debug` / `make gdb`, `make qemu`, `make qemu-debug`.**
  Commands and flags were written from Renode's and QEMU's documentation but
  none have been run. The Robot test is expected to *fail* until your kernel
  prints `kernel booted`; check that the *failure reason* is a timeout waiting
  for that line, and not a setup error.
- **SysTick and semihosting in Renode.** The stock platform does not list a
  separate SysTick (Renode implements it inside its NVIC model), and the docs do
  not make clear how to enable semihosting. Verify both early instead of
  assuming; UART output is the safe default for logging.

- **The dev container (`.devcontainer/devcontainer.json`) and the Dockerfile
  changes that came with it** (the `rust-analyzer` component and the `dev`
  user). The JSON was checked to be valid, but none of it was built or opened in
  VS Code in the environment where it was written, and no Docker was available
  there. The first "Reopen in Container" is the real test. Things worth checking
  afterwards: that `rust-analyzer` shows a version in the container terminal
  (`rust-analyzer --version`), that `src/vectors.rs` has no red errors in the
  editor, and that files created in the container are owned by your user on the
  host.

- **The container-only Makefile and the UART code (`src/uart.rs`).** The Rust
  code was type-checked against the host PC with no warnings and is
  rustfmt-clean, and a `make -n` dry run showed the expected commands. It was
  **not** built for the Cortex-M4 target or run in Renode. The UART register
  offsets were written from the nRF52840 datasheet, and Renode's UARTE model might
  not behave as expected (for example, if it never raises `EVENTS_ENDTX`, the
  first `println!` would hang). Since Step 12 this code is not compiled in.

- **The debug console (`src/debugcon.rs`, `renode/debugcon.py`, the `debugcon`
  entry in `renode/nrf52840.repl`, and the updated `renode/test.robot`).** The Rust
  was type-checked against the host PC with the feature both on and off, with no
  warnings, and the Python script was only syntax-checked. Nothing was run in
  Renode. Several things are unproven and are what to check first:
    1. **Renode accepts the peripheral.** The `.repl` syntax and the script's
       `request.IsInit` / `request.IsWrite` / `request.Value` names follow Renode's
       documentation. If `make run` fails while loading the platform, the error
       message will name the line.
    2. **The script path resolves** (`renode/debugcon.py`). If not, use an absolute
       path.
    3. **`self.InfoLog("{0}", line)` is a valid call from the script.** The
       documentation's own example calls `self.NoisyLog(...)`; `InfoLog` is assumed
       to exist alongside it. If Renode reports an error there, try
       `self.WarningLog("{0}", line)`, which Renode prints at the default log level.
    4. **Where the text appears.** Expect a line such as
       `[INFO] debugcon: kernel booted` in the Renode terminal, but that exact form
       was not seen.
    5. **`renode/test.robot`.** `Create Log Tester 5` and `Wait For Log Entry` come
       from Renode's testing documentation, which does not spell out the timeout
       units or whether matching is a substring match, so run `make test` and check
       that any failure is about the log line and not about the keyword syntax.
       If nothing prints, `pause`, check `sysbus.cpu PC`, and run `sysbus
  LogPeripheralAccess sysbus.debugcon` to see whether the CPU is reaching the
       peripheral at all.

**Important limits of simulation:** Renode and QEMU model the chip from
documentation. Code that works in simulation can still fail on real silicon,
especially for timing and unmodelled peripherals (for example SAADC, PWM, QSPI,
USBD, and CRYPTOCELL are not modelled in this platform). Core features like
exceptions, PendSV/SVC context switching, SysTick, the NVIC, and the MPU are a
good fit for simulation; anything that touches an unmodelled peripheral needs
the real board.

---

## 8. Troubleshooting

| Symptom | Likely cause and fix |
|---|---|
| `failed to connect to the docker API ... docker.sock` | The daemon is not running. `sudo systemctl enable --now docker.service`. |
| `permission denied` talking to Docker | You are not in the `docker` group. `sudo usermod -aG docker $USER`, then log out and back in. |
| Building the image fails at the Renode download | The asset name or version is wrong. Check [Renode releases](https://github.com/renode/renode/releases) for `renode-<version>.linux-portable.tar.gz` and adjust `RENODE_VERSION`. |
| Building the image fails at the SVD step | Either no internet, or the URL line in Renode's stock `nrf52840.repl` changed; check the `sed`/`grep` lines in the Dockerfile. |
| `rust-lld: error: unknown argument '-nostartfiles'` | Remove that flag from `.cargo/config.toml` (already done). |
| `PC` is a weird value like `0xE7FE`, `SP` is huge | The vector table is missing or not at address 0. Check `sysbus ReadDoubleWord 0x0` and `0x4`. |
| Renode warns `WriteDoubleWord to non existing peripheral` | A write to an address with nothing mapped: a wrong address, an unmodelled peripheral, or a garbage stack pointer. |
| Nothing appears on the UART | Make sure you use the EasyDMA UARTE registers (`TXD.PTR`, `TXD.MAXCNT`, `TASKS_STARTTX`, wait for `EVENTS_ENDTX`), and the buffer is in RAM, not flash. |
| Floating-point instruction faults (UsageFault) | The FPU is not enabled yet. Set CP10/CP11 in `CPACR` before using floats. |
| Files in `target/` owned by root | The container was run without `--user`; use the Makefile targets, which pass it. Inside the dev container, check that `remoteUser` is `dev` and `updateRemoteUserUID` is `true`. |
| rust-analyzer shows `can't find crate for 'test'` | It is analyzing test targets. Make sure `rust-analyzer.cargo.allTargets` and `rust-analyzer.check.allTargets` are `false` (set in `devcontainer.json`). |
| rust-analyzer analyzes for your PC instead of the ARM chip | Make sure it runs *inside* the dev container, and that `rust-analyzer.cargo.target` is `thumbv7em-none-eabihf`. |
| `make: renode: No such file or directory` (or `cargo`, `gdb-multiarch`) | You are running `make` on the host, not inside the container. Reopen the folder in the dev container, or use the `docker run` one-liner at the top of the `Makefile`. |
| Renode crashes or complains it cannot write a file in `$HOME` | `HOME` is not writable. Inside the dev container it is `/home/dev`, which is fine; from a plain `docker run`, pass `-e HOME=/tmp` as in the Makefile's comment. |
| Dev container build fails at `useradd` | UID 1000 is already taken in the base image. Change the UID in the Dockerfile's `useradd` line. |

---

## 9. What to do next

The environment is ready; the remaining work is yours. A sensible order:

1. **Rebuild the image** to pick up the baked-in SVD (in VS Code: "Dev Containers:
   Rebuild Container"), then confirm `make run` no longer downloads anything.
2. **Verify the vector table** using the checks in section 7.
3. **Write your reset handler.** The linker script already exports `_sdata`,
   `_edata`, `_sidata`, `_sbss`, `_ebss` and `_estack` for copying `.data`, zeroing
   `.bss`, and setting up the stack. Remember to enable the FPU if you keep the
   `hf` target.
4. **Check the debug console prints `kernel booted`.** The code already exists
   (Step 12): confirm the line appears when you `make run`, and that `make test`
   passes. Then use `dprintln!` as your debug channel for everything after, and
   build for hardware with `--no-default-features` so it compiles out.
5. **Bring up SysTick and PendSV**, the foundation of a scheduler, and check them
   in the simulator.
6. **Use `make debug` and `make gdb`** when something goes wrong: set a breakpoint
   at `ResetHandler`, step, and inspect registers. `monitor <command>` inside GDB runs
   Renode Monitor commands.
7. **Get real hardware** when you can. Your `link.ld` already matches the
   nRF52840's memory map, so the same ELF is what you will flash.

---

## 10. Sources

- [QEMU: Nordic nRF boards (microbit)](https://qemu.readthedocs.io/en/master/system/arm/nrf.html), which states that the `microbit` machine is an nRF51822 / Cortex-M0 and that nRF52 is not supported.
- [Renode releases](https://github.com/renode/renode/releases) (1.17.0 notes and assets).
- [Renode: Using Python in Renode](https://renode.readthedocs.io/en/latest/basic/using-python.html) (Python peripherals in a `.repl`).
- [Renode: Testing API](https://renode.readthedocs.io/en/latest/basic/renode-testing-api.html) (`Create Log Tester`, `Wait For Log Entry`).
- [Renode changelog](https://github.com/renode/renode/blob/master/CHANGELOG.rst) (Arm semihosting calls added in 1.17.0).
- [Renode issue #516: Semihosting UART support](https://github.com/renode/renode/issues/516).
- [Renode README](https://github.com/renode/renode/blob/master/README.md) (installation, flags, Docker).
- [Renode: Debugging with GDB](https://renode.readthedocs.io/en/latest/debugging/gdb.html).
- [Renode: UART integration](https://renode.readthedocs.io/en/latest/host-integration/uart.html).
- [Renode: Running Renode in different modes](https://renode.readthedocs.io/en/latest/basic/running.html).
- [Renode: Supported boards](https://renode.readthedocs.io/en/latest/introduction/supported-boards.html).
- [Renode's stock `nrf52840.repl`](https://raw.githubusercontent.com/renode/renode/master/platforms/cpus/nrf52840.repl).
- [Initial Bluetooth support for nRF52840 in Renode](https://www.zephyrproject.org/initial-bluetooth-support-for-nrf52840-in-renode/).