//! Quick-and-dirty debug console: print to the Renode log with no UART setup.
//!
//! `renode/nrf52840.repl` adds a tiny made-up peripheral at 0x6000_0000 (see
//! `renode/debugcon.py`). Writing a byte to it makes Renode print it. It does
//! not exist on a real nRF52840, where a write to that address bus-faults, so
//! this is gated behind the kernel's `debugcon` cargo feature (turned on by the
//! app's default features). Build for real hardware with
//! `cargo build --no-default-features`, and every `dprint!`/`dprintln!` compiles
//! to nothing.
//!
//! Privileged code only: 0x6000_0000 is outside every MPU region that threads may
//! access, so printing from an unprivileged thread faults.
//!
//! Needs no setup: no clock, pins or `.data`/`.bss`, so it works from the very
//! first instruction of `Reset`.

use core::fmt::{self, Write};

/// Whether the console is compiled in. The macros test this rather than
/// `cfg!(feature = "debugcon")`, which would be evaluated in the calling crate.
pub const ENABLED: bool = cfg!(feature = "debugcon");

/// Send one byte (`arch::log_putc` knows where the console is).
pub fn putc(byte: u8) {
    crate::arch::log_putc(byte)
}

/// Lets `core::fmt` write to the debug console.
pub struct DebugCon;

impl Write for DebugCon {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            putc(b);
        }
        Ok(())
    }
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    let _ = DebugCon.write_fmt(args);
}

/// Like `print!`, but to the debug console. A line only appears in Renode once
/// a `\n` is sent, so end messages with `dprintln!`.
#[macro_export]
macro_rules! dprint {
    ($($arg:tt)*) => {
        if $crate::debugcon::ENABLED {
            $crate::debugcon::_print(format_args!($($arg)*))
        }
    };
}

/// Like `println!`, but to the debug console.
#[macro_export]
macro_rules! dprintln {
    () => { $crate::dprint!("\n") };
    ($($arg:tt)*) => {
        if $crate::debugcon::ENABLED {
            $crate::debugcon::_print(format_args!("{}\n", format_args!($($arg)*)))
        }
    };
}
