//! Everything unprivileged thread code may call.
//!
//! This crate is linked into user flash (link.ld places only the `kernel` crate in
//! KFLASH), so threads can execute it after the MPU makes kernel flash
//! privileged-only. It talks to the kernel solely through `svc`.
//!
//! Calling convention: r0, r1 are the arguments; a return value comes back in r0.
#![no_std]

use core::arch::asm;

mod global;
pub use global::Global;

///////////////////////////////////////////////////////////////////////////////////////////////////
// Syscall numbers (shared with the kernel's SVC dispatcher)
///////////////////////////////////////////////////////////////////////////////////////////////////

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Syscall {
    Start = 0, // kernel only: drop privilege and run the first thread
    Yield = 1,
    Sleep = 2,   // r0 = milliseconds
    SemTake = 3, // r0 = semaphore handle -> r0 = 0 ok / 1 error; may block
    SemGive = 4, // r0 = semaphore handle -> r0 = 0 ok / 1 error
    Fault = 5,   // the calling thread hit an unrecoverable error (e.g. panicked)
    Invalid = u8::MAX,
}

impl TryFrom<u8> for Syscall {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Start),
            1 => Ok(Self::Yield),
            2 => Ok(Self::Sleep),
            3 => Ok(Self::SemTake),
            4 => Ok(Self::SemGive),
            5 => Ok(Self::Fault),
            _ => Err(()),
        }
    }
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Syscall wrappers
///////////////////////////////////////////////////////////////////////////////////////////////////

/// Issue syscall `$num` (optionally with `$arg` in r0) and return the
/// kernel's r0.
///
/// Every `svc` in this project must declare r0 as an output: the kernel
/// *always* writes a return value into the caller's stacked r0, even for calls
/// whose result is ignored. Otherwise the compiler may assume r0 survived the
/// `svc` (or hoist an r0 input out of a loop). r1-r3, r12 and xPSR come back
/// from the hardware-stacked frame unchanged, and PendSV preserves r4-r11.
///
/// No `options(...)` on purpose: the defaults (may read/write memory, may use
/// the stack, clobbers flags) are right for a call into the kernel, which can
/// write user memory and let other threads run before returning.
macro_rules! syscall {
    ($num:expr) => {{
        let r0: u32;
        unsafe { core::arch::asm!("svc #{n}", n = const $num as u8, lateout("r0") r0) };
        r0
    }};
    ($num:expr, $arg:expr) => {{
        let r0: u32;
        unsafe { core::arch::asm!("svc #{n}", n = const $num as u8, inout("r0") $arg => r0) };
        r0
    }};
}

#[inline(always)]
pub fn sys_yield() {
    syscall!(Syscall::Yield);
}

#[inline(always)]
pub fn sys_sleep(ms: u32) {
    syscall!(Syscall::Sleep, ms);
}

pub fn sys_sem_take(handle: u32) -> u32 {
    syscall!(Syscall::SemTake, handle)
}

pub fn sys_sem_give(handle: u32) -> u32 {
    syscall!(Syscall::SemGive, handle)
}

/// Report that the calling thread cannot continue. The kernel does not return
/// to it.
pub fn sys_fault() -> ! {
    syscall!(Syscall::Fault);
    // Guard in case the kernel ever returns.
    loop {
        core::hint::spin_loop()
    }
}

/// IPSR is readable from unprivileged code. Nonzero means we are in an exception handler.
#[inline(always)]
pub fn in_handler_mode() -> bool {
    let ipsr: u32;
    unsafe {
        asm!("mrs {}, ipsr", out(reg) ipsr, options(nomem, nostack, preserves_flags));
    }
    ipsr & 0x1FF != 0
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Semaphore handle
///////////////////////////////////////////////////////////////////////////////////////////////////

/// User handle to a kernel semaphore: just the address of the kernel object.
/// Create one with `kernel::semaphore!(count)`.
pub struct Semaphore {
    // A raw pointer, never dereferenced here: it points into kernel RAM, which
    // unprivileged code cannot read. It is only passed to the kernel as a handle.
    kobj: *const (),
}

// The handle is just an address; all shared state is in the kernel and guarded there.
unsafe impl Sync for Semaphore {}
unsafe impl Send for Semaphore {}

#[allow(dead_code)]
impl Semaphore {
    /// Used by `kernel::semaphore!`. The kernel validates the address on every
    /// syscall, so a bogus value is rejected, not trusted.
    #[doc(hidden)]
    pub const fn from_kernel_object(kobj: *const ()) -> Self {
        Self { kobj }
    }

    pub fn handle(&self) -> u32 {
        self.kobj as usize as u32
    }

    /// Blocks the calling thread if the count is exhausted. Thread context only.
    pub fn take(&self) {
        debug_assert!(!in_handler_mode(), "Semaphore::take from an ISR");
        assert!(sys_sem_take(self.handle()) == 0, "bad semaphore handle");
    }

    /// Thread context only. A handler cannot issue `svc` at the same priority as
    /// SVC (that escalates to HardFault), so ISRs use `kernel::give_from_isr`.
    pub fn give(&self) {
        debug_assert!(
            !in_handler_mode(),
            "Semaphore::give from an ISR: use kernel::give_from_isr"
        );
        assert!(sys_sem_give(self.handle()) == 0, "bad semaphore handle");
    }

    pub fn with<F>(&self, f: F)
    where
        F: FnOnce(),
    {
        self.take();
        f();
        self.give();
    }
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Idle thread
///////////////////////////////////////////////////////////////////////////////////////////////////

/// Lowest-priority thread, created by the kernel. Runs unprivileged.
///
/// `#[inline(never)]` is load-bearing: when optimising, rustc treats small
/// functions as cross-crate-inlinable and the kernel, which takes this function's
/// address, would then codegen its own copy in KFLASH. The thread would then
/// fault on its first instruction (the kernel's `is_user_text` assert catches it
/// at boot). `inline(never)` keeps the only copy here, in user flash.
#[inline(never)]
pub extern "C" fn idle_thread() -> ! {
    loop {
        unsafe { asm!("wfe") };
    }
}
