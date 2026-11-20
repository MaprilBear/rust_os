//! Host builds only (tests, Miri, fuzzers): stand-ins for the CPU features the
//! wrappers in this crate use. The kernel's `test_support` drives these; tests
//! normally don't call them directly.

use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicPtr, Ordering::SeqCst};

/// Receives `(syscall number, r0)` and returns the new r0.
pub type SyscallHandler = fn(u8, usize) -> usize;

/// The installed handler (null: none). Kept as a pointer, not an integer, so
/// its provenance survives (Miri checks that).
static HANDLER: AtomicPtr<()> = AtomicPtr::new(ptr::null_mut());
static IN_HANDLER_MODE: AtomicBool = AtomicBool::new(false);

/// Route `sys_*` calls to `handler` (None: they panic).
pub fn set_syscall_handler(handler: Option<SyscallHandler>) {
    HANDLER.store(handler.map_or(ptr::null_mut(), |h| h as *mut ()), SeqCst);
}

pub(crate) fn syscall(num: u8, arg0: usize) -> usize {
    let handler = HANDLER.load(SeqCst);
    assert!(
        !handler.is_null(),
        "syscall {num} with no host syscall handler installed"
    );
    // Safety: only `set_syscall_handler` stores non-null values, and it stores
    // `SyscallHandler`s.
    let handler = unsafe { core::mem::transmute::<*mut (), SyscallHandler>(handler) };
    handler(num, arg0)
}

/// Make `in_handler_mode()` report an exception handler (true) or thread code.
pub fn set_in_handler_mode(on: bool) {
    IN_HANDLER_MODE.store(on, SeqCst);
}

pub(crate) fn in_handler_mode() -> bool {
    IN_HANDLER_MODE.load(SeqCst)
}
