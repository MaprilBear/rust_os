//! Everything unprivileged thread code may call.
//!
//! This crate is linked into user flash (link.ld places only the `kernel` crate in
//! KFLASH), so threads can execute it after the MPU makes kernel flash
//! privileged-only. It talks to the kernel solely through `svc`.
//!
//! Calling convention: up to two arguments in r0 and r1; the return value
//! comes back in r0.
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
    /// r0 = pointer, r1 = length (at most 256) -> r0 = 0 ok / 1 the buffer is
    /// too long or not readable by the caller (user RAM or user flash).
    /// Writes the bytes to the kernel's debug console as one unit.
    DebugWrite = 6,
    /// Kernel feature `ktest` only (else 1). r0 = pointer to a word in user
    /// RAM (0: disarm), r1 = value. The calling thread's next MemManage fault
    /// writes the value, with the fault's MMFSR in bits 24-31, there and parks
    /// the thread instead of stopping the system. -> r0 = 0 ok / 1 error.
    ExpectFault = 7,
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
            6 => Ok(Self::DebugWrite),
            7 => Ok(Self::ExpectFault),
            _ => Err(()),
        }
    }
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Syscall wrappers
///////////////////////////////////////////////////////////////////////////////////////////////////

/// Issue syscall `$num` (optionally with arguments in r0 and r1) and return
/// the kernel's r0.
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
            host::syscall($num as u8, 0, 0)
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
            host::syscall($num as u8, $arg, 0)
        }
    }};
    ($num:expr, $arg0:expr, $arg1:expr) => {{
        #[cfg(target_os = "none")]
        {
            let r0: usize;
            unsafe {
                core::arch::asm!(
                    "svc #{n}",
                    n = const $num as u8,
                    inout("r0") $arg0 => r0,
                    in("r1") $arg1,
                )
            };
            r0
        }
        #[cfg(not(target_os = "none"))]
        {
            host::syscall($num as u8, $arg0, $arg1)
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

/// Write `len` bytes at `ptr` to the debug console, as one unit (nothing else
/// the kernel prints can land in the middle). 0 if written, 1 if the buffer
/// is not readable by the calling thread.
pub fn sys_debug_write(ptr: usize, len: usize) -> usize {
    syscall!(Syscall::DebugWrite, ptr, len)
}

/// Arm (or, with `ptr` 0, disarm) the kernel's expected-fault hook for the
/// calling thread. See [`Syscall::ExpectFault`].
pub fn sys_expect_fault(ptr: usize, value: usize) -> usize {
    syscall!(Syscall::ExpectFault, ptr, value)
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
    static CALLS: Mutex<Vec<(u8, usize, usize)>> = Mutex::new(Vec::new());

    fn record(num: u8, arg0: usize, arg1: usize) -> usize {
        CALLS.lock().unwrap().push((num, arg0, arg1));
        // Fail on handle/pointer 0xBAD, succeed otherwise.
        usize::from(arg0 == 0xBAD)
    }

    fn with_hook<R>(f: impl FnOnce() -> R) -> (R, Vec<(u8, usize, usize)>) {
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
            Syscall::DebugWrite,
            Syscall::ExpectFault,
        ] {
            assert_eq!(Syscall::try_from(s as u8), Ok(s));
        }
        assert_eq!(
            (0..=255u8)
                .filter(|&n| Syscall::try_from(n).is_ok())
                .count(),
            8
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
            assert_eq!(sys_debug_write(0x2000_8000, 12), 0);
            assert_eq!(sys_expect_fault(0x2000_8004, 0xF00D), 0);
        });
        assert_eq!(
            calls,
            [
                (Syscall::Yield as u8, 0, 0),
                (Syscall::Sleep as u8, 7, 0),
                (Syscall::Sleep as u8, u32::MAX as usize, 0),
                (Syscall::SemTake as u8, 0x2000_0010, 0),
                (Syscall::SemGive as u8, 0x2000_0010, 0),
                (Syscall::SemTake as u8, 0xBAD, 0),
                (Syscall::DebugWrite as u8, 0x2000_8000, 12),
                (Syscall::ExpectFault as u8, 0x2000_8004, 0xF00D),
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
                (Syscall::SemTake as u8, 0x2000_0040, 0),
                (Syscall::SemGive as u8, 0x2000_0040, 0)
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
