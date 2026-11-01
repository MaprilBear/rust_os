//! Quick-and-dirty debug console: print to the Renode log with no UART setup.
//!
//! `renode/nrf52840.repl` adds a tiny made-up peripheral at 0x6000_0000 (see
//! `renode/debugcon.py`). Writing a byte to it makes Renode print it. It does
//! not exist on a real nRF52840, where a write to that address bus-faults, so
//! this is gated behind the `debugcon` cargo feature (on by default). Build for
//! real hardware with `cargo build --no-default-features`, and every
//! `dprint!`/`dprintln!` compiles to nothing.
//!
//! Needs no setup: no clock, pins or `.data`/`.bss`, so it works from the very
//! first instruction of `Reset`.

use core::fmt::{self, Write};
use core::ptr::write_volatile;

/// Must match the address in `renode/nrf52840.repl`.
const DEBUGCON: *mut u32 = 0x6000_0000 as *mut u32;

/// Send one byte. 32-bit write, because the Renode peripheral is a single
/// 4-byte register.
pub fn putc(byte: u8) {
    unsafe { write_volatile(DEBUGCON, byte as u32) }
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
#[allow(unused_macros)]
macro_rules! dprint {
    ($($arg:tt)*) => {
        if cfg!(feature = "debugcon") {
            $crate::debugcon::_print(format_args!($($arg)*))
        }
    };
}

/// Like `println!`, but to the debug console.
#[allow(unused_macros)]
macro_rules! dprintln {
    () => { dprint!("\n") };
    ($($arg:tt)*) => {
        if cfg!(feature = "debugcon") {
            $crate::debugcon::_print(format_args!("{}\n", format_args!($($arg)*)))
        }
    };
}
