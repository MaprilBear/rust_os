#![allow(future_incompatible)]
// TODO: Add mutexes
// TODO: Make the scheduler priority based
// TODO: Support cooperative and premptive threads like Zephyr
// TODO: Implement workqueues
// TODO: Implement UART driver

#![no_std]
#![no_main]

#[macro_use]
mod debugcon;

mod reset;
mod semaphore;
mod threads;
mod uarte;
mod vectors;

extern crate alloc;

use alloc::{boxed::Box, vec::Vec};
use bitfields::bitflag;
use core::{arch::asm, cell::RefCell, ptr};
use cortex_m as _;
use embedded_alloc::TlsfHeap as Heap;

#[global_allocator]
static HEAP: Heap = Heap::empty();

#[repr(C, align(8))] // AAPCS requires the SP to be 8-byte aligned
struct Stack(Vec<u32>);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Ready,
    Running,
    Blocked,
}

#[repr(C)]
pub struct TCB {
    priority: u32,
    sp: *mut u32,
    stack: Box<Stack>,
    state: ThreadState,
    id: ThreadId,
}

type ThreadId = usize;

type TcbEntries = Vec<Option<TCB>>;

struct TcbList(RefCell<TcbEntries>);

// SAFETY: Access is only through `with`, which enters a critical section.
// This relies on the critical-section implementation providing exclusion
// for every context that can access this single-core MCU.
unsafe impl Sync for TcbList {}

impl TcbList {
    fn with<R>(&self, f: impl for<'a> FnOnce(&'a mut TcbEntries) -> R) -> R {
        critical_section::with(|_| {
            let mut entries = self.0.borrow_mut();
            f(&mut entries)
        })
    }
}

struct Scheduler {
    tcb_list: TcbList,
}

static SCHEDULER: Scheduler = Scheduler {
    tcb_list: TcbList(RefCell::new(Vec::new())),
};

impl Scheduler {
    fn schedule(&self) {
        self.tcb_list.with(|tcb_list| {
            let len = tcb_list.len();
            if len == 0 {
                return;
            }

            let start = unsafe {
                if CURR_TCB.is_null() {
                    0
                } else {
                    let current = &mut *CURR_TCB;
                    if current.state == ThreadState::Running {
                        current.state = ThreadState::Ready;
                    }
                    (current.id + 1) % len
                }
            };

            for offset in 0..len {
                let index = (start + offset) % len;
                if let Some(tcb) = tcb_list[index].as_mut() {
                    if tcb.state == ThreadState::Ready {
                        tcb.state = ThreadState::Running;
                        dprintln!("Scheduled next thread: {:p}", tcb);
                        unsafe { NEXT_TCB = tcb as *mut TCB };
                        break;
                    }
                }
            }
        });
    }
}

// Globals used to identify the current and next threads for thr context switch logic
#[no_mangle]
pub static mut CURR_TCB: *mut TCB = ptr::null_mut();

#[no_mangle]
pub static mut NEXT_TCB: *mut TCB = ptr::null_mut();

// Saved exception frame layout (for a fresh stack)
// ^ 0xFFFF_FFFF
// |
// -1   | PSR -- (top of stack)
// -2  | PC   |
// -3  | LR   |
// -4  | R12  | popped by returning from an exception
// -5  | R3   |
// -6  | R2   |
// -7  | R1   |
// -8  | R0 ---
// -9  | R4 ----
// -10  | R5    |
// -11 | R6    |
// -12 | R7    |
// -13 | R8    | popped manually by the PendSV Handler
// -14 | R9    |
// -15 | R10   |
// -16 | R11 ---   <---- current stack pointer
// |
// V 0x0000_0000
type ThreadEntry = extern "C" fn() -> !;
fn create_tcb(thread_entry: ThreadEntry, stack_size: usize) {
    let mut stack = Box::new(Stack(Vec::with_capacity(stack_size)));
    unsafe {
        let stack_top: *mut u32 = stack.0.as_mut_ptr().add(stack_size);
        let sp = stack_top.sub(16);
        let lr = stack_top.sub(3);
        let pc = stack_top.sub(2);
        let psr = stack_top.sub(1);

        // Set up the LR, PC, and PSR
        *lr = 0xFFFF_FFFF; // EXC_RETURN for standard frame, default register, thumb logic
        *pc = thread_entry as usize as u32;
        *psr = 1 << 24; // Set thumb bit

        let mut tcb = TCB {
            id: 0,
            priority: 0,
            sp,
            stack,
            state: ThreadState::Ready,
        };

        SCHEDULER.tcb_list.with(|tcb_list| {
            if let Some(slot_index) = tcb_list.iter().position(|slot| slot.is_none()) {
                tcb.id = slot_index;
                tcb_list[slot_index] = Some(tcb);
            } else {
                tcb.id = tcb_list.len();
                tcb_list.push(Some(tcb));
            }
        });
    }
}

#[bitflag(u8)]
enum Nrf52840InterruptPriority {
    #[base]
    Zero = 0x00,
    One = 0x20,
    Two = 0x40,
    Three = 0x60,
    Four = 0x80,
    Five = 0xA0,
    Six = 0xC0,
    Seven = 0xE0,
}

fn start_systick() {
    unsafe {
        let core_periph = cortex_m::peripheral::Peripherals::take().unwrap();
        let mut syst = core_periph.SYST;
        let mut scb = core_periph.SCB;

        syst.disable_interrupt();
        syst.disable_counter();
        syst.set_reload(64_000_000 / 100); // 1000 Hz (1 ms period)
        syst.clear_current();
        scb.set_priority(
            cortex_m::peripheral::scb::SystemHandler::SysTick,
            Nrf52840InterruptPriority::Six.into_bits(),
        );
        // PendSV must not preempt SysTick before the active exception can return.
        scb.set_priority(
            cortex_m::peripheral::scb::SystemHandler::PendSV,
            Nrf52840InterruptPriority::Seven.into_bits(),
        );
        syst.enable_interrupt();
        syst.enable_counter();
    }
}

fn os_init() {
    unsafe {
        embedded_alloc::init!(HEAP, 4098);
    }
}

fn os_add_thread(thread_entry: ThreadEntry, stack_size: usize) {
    create_tcb(thread_entry, stack_size);
}

fn os_start() -> ! {
    start_systick();
    os_yield();
    panic!("os_start should never return");
}

pub extern "C" fn context_switch() {
    unsafe {
        asm!(
            "LDR     R0, ={int_ctrl}    ",
            "LDR     R1, ={pendsv}      ", // Execution will leave this thread until scheduled again
            "STR     R1, [R0]",
            int_ctrl = const 0xE000_ED04u32,
            pendsv = const 0x1000_0000u32,
        )
    }
}

fn os_yield() {
    SCHEDULER.schedule();
    context_switch();
}
