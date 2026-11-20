//! MPU attacks: an unprivileged thread touching what only the kernel may
//! must take a MemManage fault (each test expects one, and the kernel parks
//! the thread), and what threads may touch must work.
//!
//! Each test checks the kind of fault (MMFSR): data access for loads and
//! stores, instruction fetch for executing.
//!
//! Not covered: the System Control Space (NVIC, SysTick, MPU registers at
//! 0xE000_xxxx). On hardware unprivileged access there is a BusFault, which
//! escalates to HardFault and stops the system; Renode reports a MemManage
//! fault instead, so a test there would pass in Renode and abort on the DK.
#![no_std]
#![no_main]

use ktest::{expect_fault_on, tests, Access};
use userlib::Semaphore;

static SEM: Semaphore = kernel::semaphore!(1);
static FLASH_WORD: u32 = 0x1234_5678;
static mut RAM_WORD: u32 = 0;
/// `bx lr`, to try executing from RAM.
static mut RAM_CODE: [u16; 2] = [0x4770, 0x4770];

#[no_mangle]
pub extern "C" fn user_main() {
    ktest::start(
        "kernel_mpu",
        tests![
            user_ram_flash_and_peripherals_are_accessible,
            reading_kernel_ram_faults,
            writing_kernel_ram_faults,
            kernel_ram_ends_where_user_ram_starts,
            kernel_flash_ends_where_user_flash_starts,
            forging_a_semaphore_object_faults,
            reading_kernel_flash_faults,
            executing_kernel_code_faults,
            writing_flash_faults,
            writing_the_vector_table_faults,
            executing_from_ram_faults,
            the_debug_console_is_privileged,
        ],
    );
}

fn read(addr: usize) {
    unsafe { core::ptr::read_volatile(addr as *const u32) };
}
fn write(addr: usize) {
    unsafe { core::ptr::write_volatile(addr as *mut u32, 0xBAD) };
}

fn user_ram_flash_and_peripherals_are_accessible() {
    unsafe {
        let ram = &raw mut RAM_WORD;
        ram.write_volatile(7);
        assert_eq!(ram.read_volatile(), 7);
        assert_eq!(core::ptr::read_volatile(&FLASH_WORD), 0x1234_5678);
        // EGU0 INTEN: a peripheral register (the MPU's device region).
        core::ptr::read_volatile(0x4001_4300 as *const u32);
    }
}

/// Kernel RAM is 0x2000_0000..0x2000_8000 (link.ld, MPU region 3).
const KRAM_END: usize = 0x2000_8000;
/// Kernel flash is 0x0..0x1_0000 (link.ld, MPU region 1).
const KFLASH_END: usize = 0x1_0000;

fn reading_kernel_ram_faults() {
    expect_fault_on(Access::Data, || read(0x2000_4000));
}

fn writing_kernel_ram_faults() {
    expect_fault_on(Access::Data, || write(0x2000_4000));
}

/// The last word of kernel RAM faults; the first word of user RAM (read
/// only, it holds user data) does not.
fn kernel_ram_ends_where_user_ram_starts() {
    read(KRAM_END);
    expect_fault_on(Access::Data, || read(KRAM_END - 4));
}

fn kernel_flash_ends_where_user_flash_starts() {
    read(KFLASH_END);
    expect_fault_on(Access::Data, || read(KFLASH_END - 4));
}

/// A semaphore's kernel object lives in kernel RAM (.ksem): a thread cannot
/// forge its count or wait queue.
fn forging_a_semaphore_object_faults() {
    let object = SEM.handle();
    expect_fault_on(Access::Data, || write(object));
}

fn reading_kernel_flash_faults() {
    expect_fault_on(Access::Data, || read(0x0000_0200));
}

/// The fetch of the kernel function's first instruction faults (were it to
/// run, its SysTick read would fault too, but as a data access).
fn executing_kernel_code_faults() {
    let kernel_fn: fn() -> u32 = kernel::os_get_frc;
    expect_fault_on(Access::Instruction, || {
        core::hint::black_box(kernel_fn)();
    });
}

fn writing_flash_faults() {
    let user_flash = &FLASH_WORD as *const u32 as usize;
    expect_fault_on(Access::Data, || write(user_flash));
}

fn writing_the_vector_table_faults() {
    expect_fault_on(Access::Data, || write(0x0000_0008));
}

fn executing_from_ram_faults() {
    let code = (&raw const RAM_CODE as usize) | 1; // Thumb
    let f: extern "C" fn() = unsafe { core::mem::transmute(code) };
    expect_fault_on(Access::Instruction, || core::hint::black_box(f)());
}

fn the_debug_console_is_privileged() {
    expect_fault_on(Access::Data, || write(0x6000_0000));
}
