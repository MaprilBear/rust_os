//! System calls (SVC), kernel side.
//!
//! Thread mode runs unprivileged, so it cannot write ICSR (to pend PendSV), touch
//! the scheduler or KRAM, or poke the System Control Space. It reaches the kernel
//! only through `svc #n`; the handler runs in handler mode (always privileged).
//!
//! Calling convention: r0, r1 are the arguments; a return value goes back in r0.
//!
//! The user-side wrappers (`sys_*`) and the `Syscall` numbers live in `userlib`,
//! so threads never execute code from this crate.

use core::arch::global_asm;
use core::ptr::addr_of;
use core::sync::atomic::AtomicBool;

use userlib::Syscall;

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

fn in_range(ptr: usize, len: usize, base: usize, size: usize) -> bool {
    ptr >= base && len <= size && ptr - base <= size - len
}

/// The kernel must not trust pointers from user code: the MPU does not stop a
/// handler from dereferencing them.
#[allow(dead_code)]
fn user_can_read(ptr: usize, len: usize) -> bool {
    let flash_base = addr_of!(_flash_base) as usize;
    let flash_size = addr_of!(_flash_size) as usize;
    in_range(ptr, len, flash_base, flash_size) || user_can_write(ptr, len)
}

#[allow(dead_code)]
fn user_can_write(ptr: usize, len: usize) -> bool {
    let uram_base = addr_of!(_uram_base) as usize;
    let uram_size = addr_of!(_uram_size) as usize;
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

    /// The calling thread cannot continue (e.g. it panicked). There is no thread
    /// teardown yet, so halt like the MemManage handler does.
    pub(super) fn fault() -> ! {
        match unsafe { scheduler().current_thread() } {
            Some(id) => dprintln!("thread {} faulted", id),
            None => dprintln!("fault reported with no running thread"),
        }
        #[allow(clippy::empty_loop)]
        loop {}
    }

    pub(super) fn start(exc_return: u32) -> usize {
        // Only the boot code may do this: it must come from privileged
        // thread mode on MSP (EXC_RETURN low nibble 0x9), and only once.
        if exc_return & 0xF != 0x9 || STARTED.swap(true, Ordering::SeqCst) {
            return 1;
        }
        start_systick();
        os_yield();
        0
    }

    pub(super) fn r#yield() -> usize {
        os_yield();
        0
    }

    pub(super) fn sleep(ms: u32) -> usize {
        // Only a running thread can sleep (not boot code).
        if !unsafe { scheduler().sleep(ms) } {
            return 1;
        }
        os_yield();
        0
    }

    pub(super) fn sem_take(handle: usize) -> usize {
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

    pub(super) fn sem_give(handle: usize) -> usize {
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
    let arg0 = unsafe { frame.read() } as usize;
    let num = unsafe { ((pc - 2) as *const u8).read() };
    let syscall_num = Syscall::try_from(num).unwrap_or(Syscall::Invalid);

    let r0 = match syscall_num {
        Syscall::Start => syscall_impl::start(exc_return),
        Syscall::Yield => syscall_impl::r#yield(),
        Syscall::Sleep => syscall_impl::sleep(arg0 as u32),
        Syscall::SemTake => syscall_impl::sem_take(arg0),
        Syscall::SemGive => syscall_impl::sem_give(arg0),
        Syscall::Fault => syscall_impl::fault(),
        Syscall::Invalid => {
            dprintln!("Invalid SVC number {num}");
            0
        }
    };

    unsafe { frame.write(r0 as u32) };
}
