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

mod global;
mod reset;
mod scheduler;
mod semaphore;
mod simple_list;
mod threads;
mod uarte;
mod vectors;

extern crate alloc;

use alloc::{boxed::Box, vec::Vec};
use bitfields::bitflag;
use core::arch::asm;
use cortex_m as _;
use embedded_alloc::TlsfHeap as Heap;

use crate::scheduler::{scheduler, Scheduler};

#[global_allocator]
static HEAP: Heap = Heap::empty();

#[repr(C, align(8))] // AAPCS requires the SP to be 8-byte aligned
struct Stack(Vec<u32>);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Ready,
    Running,
    Blocked,
    Sleeping,
}

#[repr(C)]
pub struct TCB {
    priority: u8,
    sp: *mut u32,
    stack: Box<Stack>,
    state: ThreadState,
    id: ThreadId,
    sleep_ms: u32,
}

type ThreadId = usize;

fn os_tick() {
    unsafe { scheduler().tick() };
    os_yield();
}

#[no_mangle]
pub extern "C" fn SystickHandler() {
    //dprintln!("SysTick!");
    os_tick();
}

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
fn create_tcb(thread_entry: ThreadEntry, stack_size: usize, priority: u8) {
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

        scheduler().add_thread(TCB {
            id: 0,
            priority,
            sp,
            stack,
            state: ThreadState::Ready,
            sleep_ms: 0,
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

use crate::vectors::core_periph;

fn start_systick() {
    unsafe {
        let core_periph = core_periph();

        core_periph.SYST.disable_interrupt();
        core_periph.SYST.set_reload(64_000_000 / 1000); // 1000 Hz (1 ms period) tuned for renode
        core_periph.SYST.clear_current();
        core_periph.SCB.set_priority(
            cortex_m::peripheral::scb::SystemHandler::SysTick,
            Nrf52840InterruptPriority::Six.into_bits(),
        );
        // PendSV must not preempt SysTick before the active exception can return.
        core_periph.SCB.set_priority(
            cortex_m::peripheral::scb::SystemHandler::PendSV,
            Nrf52840InterruptPriority::Seven.into_bits(),
        );
        core_periph.SYST.enable_interrupt();
        core_periph.SYST.enable_counter();
    }
}

extern "C" fn idle_thread() -> ! {
    loop {
        os_yield();
    }
}

fn os_init() {
    unsafe {
        embedded_alloc::init!(HEAP, 16000);
    }
    os_add_thread(idle_thread, 1024, 7);
}

fn os_add_thread(thread_entry: ThreadEntry, stack_size: usize, priority: u8) {
    create_tcb(thread_entry, stack_size, priority);
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
    unsafe { scheduler().schedule() };
    context_switch();
}

fn os_sleep(ms: u32) {
    unsafe { scheduler().sleep(ms) };
    os_yield();
}
