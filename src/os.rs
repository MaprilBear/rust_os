// TODO: Add mutexes
// TODO: Make the scheduler priority based
// TODO: Support cooperative and premptive threads like Zephyr
// TODO: Implement workqueues
// TODO: Implement UART driver

#![no_std]
#![no_main]

mod nrf52840;
mod vectors;
#[macro_use]
mod debugcon;
mod exception_routines;
mod uarte;

extern crate alloc;

use crate::exception_routines::os_first_switch;
use alloc::{boxed::Box, vec::Vec};
use bitfields::bitflag;
use core::{cell::RefCell, ptr};
use cortex_m as _;
use embedded_alloc::TlsfHeap as Heap;

#[global_allocator]
static HEAP: Heap = Heap::empty();

#[repr(C, align(8))] // AAPCS requires the SP to be 8-byte aligned
struct Stack(Vec<u32>);

#[repr(C)]
pub struct TCB {
    priority: u32,
    sp: *mut u32,
    stack: Box<Stack>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ThreadId(u16);

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
            if unsafe { CURR_TCB.is_null() } {
                // No current thread, schedule the first one available
                unsafe {
                    NEXT_TCB =
                        tcb_list.iter_mut().find_map(|slot| slot.as_mut()).unwrap() as *mut TCB
                };
            } else {
                // Iterate through the list, starting the index after the current thread, and
                // schedule the next available thread
                unsafe {
                    let curr_index = tcb_list
                        .iter_mut()
                        .position(|slot| ptr::eq(slot.as_mut().unwrap() as *mut TCB, CURR_TCB))
                        .unwrap();
                    let next_index = tcb_list
                        .iter_mut()
                        .skip(curr_index + 1)
                        .position(|slot| slot.is_some());

                    if let Some(next_index) = next_index {
                        NEXT_TCB =
                            tcb_list[curr_index + 1 + next_index].as_mut().unwrap() as *mut TCB;
                    } else {
                        // No next thread found, schedule the first one available
                        NEXT_TCB =
                            tcb_list.iter_mut().find_map(|slot| slot.as_mut()).unwrap() as *mut TCB;
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

        let tcb = TCB {
            priority: 0,
            sp,
            stack,
        };

        SCHEDULER.tcb_list.with(|tcb_list| {
            if let Some(slot_index) = tcb_list.iter().position(|slot| slot.is_none()) {
                tcb_list[slot_index] = Some(tcb);
            } else {
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
        syst.set_reload(64_000_000 / 1000); // 1000 Hz (1 ms period)
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

fn os_schedule() {
    SCHEDULER.schedule();
}

fn os_start() -> ! {
    start_systick();
    os_schedule();
    os_first_switch();
}
