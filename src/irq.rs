use core::arch::asm;

#[derive(Clone, Copy)]
pub struct IrqState(u32);

/// Unconditionally disable all interrupts (`cpsid i`).
#[inline(always)]
pub fn disable() {
    unsafe {
        asm!("cpsid i", options(nostack, preserves_flags));
    }
}

/// Unconditionally enable all interrupts (`cpsie i`).
///
/// Do NOT call this to leave a critical section that might be nested; use
/// [`restore`] instead, so an outer section stays masked.
#[inline(always)]
pub fn enable() {
    unsafe {
        asm!("cpsie i", options(nostack, preserves_flags));
    }
}

/// Disable interrupts and return the previous state. Pair with [`restore`].
#[inline(always)]
pub fn disable_save() -> IrqState {
    let primask: u32;
    unsafe {
        asm!(
        "mrs {}, PRIMASK",
        "cpsid i",
        out(reg) primask,
        options(nostack, preserves_flags),
        );
    }
    IrqState(primask)
}

/// Restore the state returned by [`disable_save`]. Interrupts are only
/// re-enabled if they were enabled when the state was saved.
#[inline(always)]
pub fn restore(state: IrqState) {
    if state.0 & 1 == 0 {
        enable()
    }
}

/// Run `f` with interrupts disabled, then restore the previous state.
#[inline(always)]
pub fn critical<R>(f: impl FnOnce() -> R) -> R {
    let state = disable_save();
    let r = f();
    restore(state);
    r
}
