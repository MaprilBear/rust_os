//! System calls (SVC).
//!
//! Thread mode runs unprivileged, so it cannot write ICSR (to pend PendSV), touch
//! the scheduler or KRAM, or poke the System Control Space. It reaches the kernel
//! only through `svc #n`; the handler runs in handler mode (always privileged).
//!
//! Calling convention: r0, r1 are the arguments; a return value goes back in r0.

use core::arch::{asm, global_asm};
use core::ptr::addr_of;
use core::sync::atomic::AtomicBool;

#[repr(u8)]
enum Syscall {
    Start = 0, // kernel only: drop privilege and run the first thread
    Yield = 1,
    Sleep = 2,   // r0 = milliseconds
    SemTake = 3, // r0 = semaphore handle -> r0 = 0 ok / 1 error; may block
    SemGive = 4, // r0 = semaphore handle -> r0 = 0 ok / 1 error
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
            _ => Err(()),
        }
    }
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// User-side wrappers. `inline(always)` so they are part of the caller's own code.
///////////////////////////////////////////////////////////////////////////////////////////////////

#[inline(always)]
pub fn sys_yield() {
    unsafe { asm!("svc #{0}", const Syscall::Yield as u32) };
}

#[inline(always)]
pub fn sys_sleep(ms: u32) {
    unsafe { asm!("svc #{0}", const Syscall::Sleep as u32, in("r0") ms) };
}

pub fn sys_sem_take(handle: u32) -> u32 {
    let r0: u32;
    unsafe { asm!("svc #{0}", const Syscall::SemTake as u32, inout("r0") handle => r0) };
    r0
}

pub fn sys_sem_give(handle: u32) -> u32 {
    let r0: u32;
    unsafe { asm!("svc #{0}", const Syscall::SemGive as u32, inout("r0") handle => r0) };
    r0
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Kernel side
///////////////////////////////////////////////////////////////////////////////////////////////////

// SVCall entry. Picks the stack the caller was using (EXC_RETURN bit 2), then
// branches (not calls) to the Rust dispatcher with r0 = stacked frame and
// r1 = EXC_RETURN. LR is untouched, so the dispatcher's own return performs the
// exception return.
global_asm!(
    ".global SvcHandler",
    ".type SvcHandler, %function",
    ".thumb_func",
    "SvcHandler:",
    "tst lr, #4",
    "ite eq",
    "mrseq r0, msp",
    "mrsne r0, psp",
    "mov r1, lr",
    "b svc_dispatch",
);

static STARTED: AtomicBool = AtomicBool::new(false);

unsafe extern "C" {
    // Defined in link.ld.
    static _flash_base: u8;
    static _flash_size: u8;
    static _uram_base: u8;
    static _uram_size: u8;
}

fn in_range(ptr: u32, len: u32, base: u32, size: u32) -> bool {
    ptr >= base && len <= size && ptr - base <= size - len
}

/// The kernel must not trust pointers from user code: the MPU does not stop a
/// handler from dereferencing them.
#[allow(dead_code)]
fn user_can_read(ptr: u32, len: u32) -> bool {
    let flash_base = addr_of!(_flash_base) as u32;
    let flash_size = addr_of!(_flash_size) as u32;
    in_range(ptr, len, flash_base, flash_size) || user_can_write(ptr, len)
}

#[allow(dead_code)]
fn user_can_write(ptr: u32, len: u32) -> bool {
    let uram_base = addr_of!(_uram_base) as u32;
    let uram_size = addr_of!(_uram_size) as u32;
    in_range(ptr, len, uram_base, uram_size)
}

mod syscall_impl {
    use core::sync::atomic::Ordering;

    use crate::{
        os_yield,
        scheduler::{scheduler, Scheduler},
        semaphore::{ksem_give, ksem_take, TakeResult},
        start_systick,
        syscall::STARTED,
    };

    pub(super) fn start(exc_return: u32) -> u32 {
        // Only the boot code may do this: it must come from privileged
        // thread mode on MSP (EXC_RETURN low nibble 0x9), and only once.
        if exc_return & 0xF != 0x9 || STARTED.swap(true, Ordering::SeqCst) {
            return 1;
        }
        start_systick();
        os_yield();
        0
    }

    pub(super) fn r#yield() -> u32 {
        os_yield();
        0
    }

    pub(super) fn sleep(ms: u32) -> u32 {
        unsafe { scheduler().sleep(ms) };
        os_yield();
        0
    }

    pub(super) fn sem_take(handle: u32) -> u32 {
        match ksem_take(handle) {
            TakeResult::Acquired => 0,
            TakeResult::Error => 1,
            TakeResult::Blocked => {
                // The status is set now; the thread resumes after its `svc`
                // once `give` wakes it, with nothing left to retry.
                os_yield();
                0
            }
        }
    }

    pub(super) fn sem_give(handle: u32) -> u32 {
        match ksem_give(handle) {
            Some(woke) => {
                if woke {
                    os_yield();
                }
                0
            }
            None => 1,
        }
    }
}

#[no_mangle]
extern "C" fn svc_dispatch(frame: *mut u32, exc_return: u32) {
    // Stacked frame: r0 r1 r2 r3 r12 LR PC xPSR. The SVC number is the byte before the stacked PC.
    let pc = unsafe { frame.add(6).read() };
    let arg0 = unsafe { frame.read() };
    let num = unsafe { ((pc - 2) as *const u8).read() };
    let syscall_num = Syscall::try_from(num).unwrap_or(Syscall::Invalid);

    let r0 = match syscall_num {
        Syscall::Start => syscall_impl::start(exc_return),
        Syscall::Yield => syscall_impl::r#yield(),
        Syscall::Sleep => syscall_impl::sleep(arg0),
        Syscall::SemTake => syscall_impl::sem_take(arg0),
        Syscall::SemGive => syscall_impl::sem_give(arg0),
        Syscall::Invalid => {
            dprintln!("Invalid SVC number {num}");
            0
        }
    };

    unsafe { frame.write(r0) };
}
