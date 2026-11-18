use core::arch::asm;
use core::panic::PanicInfo;

/// The single panic handler for the image. Where it runs decides what it may do:
///
///   * unprivileged thread: it cannot print (the debug console is outside every
///     MPU region threads may access) or touch kernel state, so it asks the
///     kernel to deal with the thread via `sys_fault`.
///   * privileged (ISR, kernel, or boot code before the scheduler): print and halt.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if !userlib::in_handler_mode() && thread_is_unprivileged() {
        userlib::sys_fault();
    }
    kernel::dprintln!("Panic: {}", info);
    #[allow(clippy::empty_loop)]
    loop {}
}

/// CONTROL.nPRIV (bit 0). Only meaningful in thread mode.
fn thread_is_unprivileged() -> bool {
    let control: u32;
    unsafe { asm!("mrs {}, control", out(reg) control, options(nomem, nostack, preserves_flags)) };
    control & 1 != 0
}
