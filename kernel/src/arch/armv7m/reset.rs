use core::{
    arch::asm,
    ptr::{addr_of, addr_of_mut},
};

use super::vectors::CORE_PERIPHERALS;
use crate::{os_init, os_start};

unsafe extern "C" {
    /// Provided by the application (`#[no_mangle] pub extern "C" fn user_main()`).
    /// Runs once, privileged, on MSP, after the kernel is initialised and before
    /// the scheduler starts. It registers threads (`os_add_thread`), interrupt
    /// handlers (`irq_attach`) and initialises drivers.
    fn user_main();

    // Kernel sections (KFLASH -> KRAM)
    static _sidata: u32;
    static mut _sdata: u32;
    static mut _edata: u32;
    static mut _sbss: u32;
    static mut _ebss: u32;
    // User sections (UFLASH -> URAM)
    static _siudata: u32;
    static mut _sudata: u32;
    static mut _eudata: u32;
    static mut _subss: u32;
    static mut _eubss: u32;
}

/// Copy `[dst, end)` from `src`, 4 words per iteration.
///
/// # Safety
/// All three pointers must be word-aligned and the ranges valid.
#[inline(never)]
unsafe fn copy_words(mut src: *const u32, mut dst: *mut u32, end: *mut u32) {
    while end.offset_from(dst) >= 4 {
        asm!(
            "ldr {t0}, [{s}]",
            "ldr {t1}, [{s}, #4]",
            "ldr {t2}, [{s}, #8]",
            "ldr {t3}, [{s}, #12]",
            "str {t0}, [{d}]",
            "str {t1}, [{d}, #4]",
            "str {t2}, [{d}, #8]",
            "str {t3}, [{d}, #12]",
            "add.w {s}, {s}, #16",
            "add.w {d}, {d}, #16",
            s = inout(reg) src,
            d = inout(reg) dst,
            t0 = out(reg) _,
            t1 = out(reg) _,
            t2 = out(reg) _,
            t3 = out(reg) _,
            options(nostack)
        );
    }
    while dst < end {
        core::ptr::write_volatile(dst, core::ptr::read_volatile(src));
        dst = dst.add(1);
        src = src.add(1);
    }
}

/// Zero `[start, end)`, 4 words per iteration.
///
/// # Safety
/// Both pointers must be word-aligned and the range valid.
#[inline(never)]
unsafe fn zero_words(mut bss: *mut u32, end: *mut u32) {
    while end.offset_from(bss) >= 4 {
        asm!(
            "movs {t0}, #0",
            "movs {t1}, #0",
            "movs {t2}, #0",
            "movs {t3}, #0",
            "str {t0}, [{d}]",
            "str {t1}, [{d}, #4]",
            "str {t2}, [{d}, #8]",
            "str {t3}, [{d}, #12]",
            "add.w {d}, {d}, #16",
            d = inout(reg) bss,
            t0 = out(reg) _,
            t1 = out(reg) _,
            t2 = out(reg) _,
            t3 = out(reg) _,
            options(nostack)
        );
    }
    while bss < end {
        core::ptr::write_volatile(bss, 0);
        bss = bss.add(1);
    }
}

/// Copy `.data`/`.user_data` from flash to RAM and zero `.bss`/`.user_bss`.
///
/// # Safety
/// Must be called once, before any Rust code that reads statics, and before the
/// MPU is enabled. The linker script must word-align all the symbols above.
#[inline(never)]
unsafe fn initialize_memory() {
    // Kernel
    copy_words(
        addr_of!(_sidata),
        addr_of_mut!(_sdata),
        addr_of_mut!(_edata),
    );
    zero_words(addr_of_mut!(_sbss), addr_of_mut!(_ebss));
    // User
    copy_words(
        addr_of!(_siudata),
        addr_of_mut!(_sudata),
        addr_of_mut!(_eudata),
    );
    zero_words(addr_of_mut!(_subss), addr_of_mut!(_eubss));
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// MPU Setup
///////////////////////////////////////////////////////////////////////////////////////////////////
const MPU_CTRL: *mut u32 = 0xE000ED94u32 as _;
const MPU_RNR: *mut u32 = 0xE000ED98u32 as _;
const MPU_RBAR: *mut u32 = 0xE000ED9Cu32 as _;
const MPU_RASR: *mut u32 = 0xE000EDA0u32 as _;
const SHCSR: *mut u32 = 0xE000ED24u32 as _;

// RASR fields
const fn rasr(size_log2: u32, ap: u32, xn: bool, attrs: u32) -> u32 {
    ((xn as u32) << 28) | (ap << 24) | attrs | ((size_log2 - 1) << 1) | 1
}
const SRAM: u32 = (1 << 18) | (1 << 17); // S=1, C=1, B=0 (TEX=0)
const DEV: u32 = (1 << 18) | (1 << 16); // S=1, B=1 (shareable device)
const FLASH: u32 = 1 << 17; // C=1

unsafe fn region(n: u32, base: u32, rasr_val: u32) {
    MPU_RNR.write_volatile(n);
    MPU_RBAR.write_volatile(base); // VALID/REGION bits unused, RNR selects
    MPU_RASR.write_volatile(rasr_val);
}

unsafe fn disable_region(n: u32) {
    MPU_RNR.write_volatile(n);
    MPU_RASR.write_volatile(0);
}

/// true: kernel flash (KFLASH, which holds only this crate, see link.ld) is
///   privileged-only. Unprivileged threads may only execute user flash: the
///   `userlib`/`drivers`/`app` crates and shared dependencies (`core`, `alloc`,
///   `compiler_builtins`, ...), which the linker places there. A thread that
///   calls into the kernel crate takes a MemManage fault; the stacked PC/MMFAR
///   show where.
/// false: kernel code is readable/executable by threads too (still immutable,
///   and it runs with the caller's privilege). Only useful for debugging.
const STRICT_KERNEL_FLASH: bool = true;

/// Keep the RAM/flash windows in sync with link.ld (the linker script ASSERTs
/// on KFLASH and KRAM).
pub unsafe fn mpu_init() {
    region(0, 0x0000_0000, rasr(20, 0b110, false, FLASH)); // 1 MiB flash, RO for everyone
    if STRICT_KERNEL_FLASH {
        region(1, 0x0000_0000, rasr(16, 0b101, false, FLASH)); // 64 KiB kernel flash, priv RO
    } else {
        disable_region(1);
    }
    region(2, 0x2000_0000, rasr(18, 0b011, true, SRAM)); // 256 KiB RAM, RW for everyone
    region(3, 0x2000_0000, rasr(15, 0b001, true, SRAM)); // 32 KiB kernel RAM, priv RW
    region(4, 0x4000_0000, rasr(29, 0b011, true, DEV)); // peripherals
    for n in 5..8 {
        disable_region(n);
    }

    SHCSR.write_volatile(SHCSR.read_volatile() | (1 << 16)); // MEMFAULTENA
    MPU_CTRL.write_volatile(0b101); // ENABLE | PRIVDEFENA
    core::arch::asm!("dsb", "isb");
}

/// Boot runs privileged on MSP. Order matters:
///   1. initialise RAM (MPU still off, so it can write everything)
///   2. enable the MPU
///   3. `os_init`, then the application's `user_main` creates threads (the kernel
///      allocates their stacks from the user heap) and initialises drivers
///   4. `os_start`, which traps into SVC; handler mode sets CONTROL.nPRIV and
///      PendSV exception-returns into the first thread. After that, thread mode
///      is unprivileged forever and this function is never run again.
#[no_mangle]
pub extern "C" fn ResetHandler() -> ! {
    dprintln!("entered reset");
    unsafe { initialize_memory() };
    unsafe { mpu_init() };
    unsafe {
        *CORE_PERIPHERALS.as_ptr() = Some(cortex_m::peripheral::Peripherals::take().unwrap())
    };
    os_init();
    unsafe { user_main() };
    dprintln!("kernel booted");
    os_start();
}
