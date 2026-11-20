//! Everything unprivileged thread code may call.
//!
//! This crate is linked into user flash (link.ld places only the `kernel` crate in
//! KFLASH), so threads can execute it after the MPU makes kernel flash
//! privileged-only. It talks to the kernel solely through `svc`.
//!
//! Calling convention: the argument (if any) goes in r0; the return value comes
//! back in r0.
//!
//! Also builds for a host OS (tests): see [`host`].
#![no_std]

#[cfg(target_os = "none")]
use core::arch::asm;

mod global;
pub use global::Global;

#[cfg(not(target_os = "none"))]
pub mod host;

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
///
/// On a host build (tests) there is no `svc`: the call goes to the handler
/// installed with `host::set_syscall_handler` (the kernel's test support).
macro_rules! syscall {
    ($num:expr) => {{
        #[cfg(target_os = "none")]
        {
            let r0: usize;
            unsafe { core::arch::asm!("svc #{n}", n = const $num as u8, lateout("r0") r0) };
            r0
        }
        #[cfg(not(target_os = "none"))]
        {
            host::syscall($num as u8, 0)
        }
    }};
    ($num:expr, $arg:expr) => {{
        #[cfg(target_os = "none")]
        {
            let r0: usize;
            unsafe { core::arch::asm!("svc #{n}", n = const $num as u8, inout("r0") $arg => r0) };
            r0
        }
        #[cfg(not(target_os = "none"))]
        {
            host::syscall($num as u8, $arg)
        }
    }};
}

#[inline(always)]
pub fn sys_yield() {
    syscall!(Syscall::Yield);
}

#[inline(always)]
pub fn sys_sleep(ms: u32) {
    syscall!(Syscall::Sleep, ms as usize);
}

pub fn sys_sem_take(handle: usize) -> usize {
    syscall!(Syscall::SemTake, handle)
}

pub fn sys_sem_give(handle: usize) -> usize {
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
    #[cfg(target_os = "none")]
    {
        let ipsr: u32;
        unsafe {
            asm!("mrs {}, ipsr", out(reg) ipsr, options(nomem, nostack, preserves_flags));
        }
        ipsr & 0x1FF != 0
    }
    #[cfg(not(target_os = "none"))]
    {
        host::in_handler_mode()
    }
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

    pub fn handle(&self) -> usize {
        self.kobj as usize
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
        #[cfg(target_os = "none")]
        unsafe {
            asm!("wfe")
        };
        #[cfg(not(target_os = "none"))]
        core::hint::spin_loop();
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use std::sync::Mutex;
    use std::vec::Vec;

    /// The host syscall hook is global: serialise the tests that use it.
    static HOOK: Mutex<()> = Mutex::new(());
    static CALLS: Mutex<Vec<(u8, usize)>> = Mutex::new(Vec::new());

    fn record(num: u8, arg: usize) -> usize {
        CALLS.lock().unwrap().push((num, arg));
        // Fail SemTake/SemGive on handle 0xBAD, succeed otherwise.
        usize::from(arg == 0xBAD)
    }

    fn with_hook<R>(f: impl FnOnce() -> R) -> (R, Vec<(u8, usize)>) {
        let _g = HOOK.lock().unwrap_or_else(|e| e.into_inner());
        CALLS.lock().unwrap().clear();
        host::set_syscall_handler(Some(record));
        let r = f();
        host::set_syscall_handler(None);
        (r, core::mem::take(&mut *CALLS.lock().unwrap()))
    }

    #[test]
    fn syscall_numbers_round_trip() {
        for s in [
            Syscall::Start,
            Syscall::Yield,
            Syscall::Sleep,
            Syscall::SemTake,
            Syscall::SemGive,
            Syscall::Fault,
        ] {
            assert_eq!(Syscall::try_from(s as u8), Ok(s));
        }
        assert_eq!(
            (0..=255u8)
                .filter(|&n| Syscall::try_from(n).is_ok())
                .count(),
            6
        );
        assert_eq!(Syscall::try_from(Syscall::Invalid as u8), Err(()));
    }

    #[test]
    fn wrappers_pass_number_and_argument() {
        let (_, calls) = with_hook(|| {
            sys_yield();
            sys_sleep(7);
            sys_sleep(u32::MAX);
            assert_eq!(sys_sem_take(0x2000_0010), 0);
            assert_eq!(sys_sem_give(0x2000_0010), 0);
            assert_eq!(sys_sem_take(0xBAD), 1, "the kernel's r0 comes back");
        });
        assert_eq!(
            calls,
            [
                (Syscall::Yield as u8, 0),
                (Syscall::Sleep as u8, 7),
                (Syscall::Sleep as u8, u32::MAX as usize),
                (Syscall::SemTake as u8, 0x2000_0010),
                (Syscall::SemGive as u8, 0x2000_0010),
                (Syscall::SemTake as u8, 0xBAD),
            ]
        );
    }

    #[test]
    fn semaphore_handle_is_the_kernel_object_address() {
        let sem = Semaphore::from_kernel_object(0x2000_0040 as *const ());
        assert_eq!(sem.handle(), 0x2000_0040);
        let (_, calls) = with_hook(|| sem.with(|| {}));
        assert_eq!(
            calls,
            [
                (Syscall::SemTake as u8, 0x2000_0040),
                (Syscall::SemGive as u8, 0x2000_0040)
            ]
        );
    }

    #[test]
    fn semaphore_methods_panic_when_the_kernel_rejects_the_handle() {
        let sem = Semaphore::from_kernel_object(0xBAD as *const ());
        let (r, _) = with_hook(|| {
            let take = std::panic::catch_unwind(|| sem.take());
            let give = std::panic::catch_unwind(|| sem.give());
            (take.is_err(), give.is_err())
        });
        assert_eq!(r, (true, true));
    }

    #[test]
    fn syscall_without_a_handler_panics() {
        let _g = HOOK.lock().unwrap_or_else(|e| e.into_inner());
        host::set_syscall_handler(None);
        let r = std::panic::catch_unwind(sys_yield);
        assert!(r.is_err());
    }

    #[test]
    fn handler_mode_flag_is_settable() {
        let _g = HOOK.lock().unwrap_or_else(|e| e.into_inner());
        assert!(!in_handler_mode());
        host::set_in_handler_mode(true);
        assert!(in_handler_mode());
        host::set_in_handler_mode(false);
        assert!(!in_handler_mode());
    }

    #[test]
    fn global_reads_and_writes_through_its_pointer() {
        static G: Global<[u8; 4]> = Global::new([1, 2, 3, 4]);
        assert_eq!(G.read(), [1, 2, 3, 4]);
        unsafe { (*G.as_ptr())[2] = 9 };
        assert_eq!(G.read(), [1, 2, 9, 4]);
    }
}
