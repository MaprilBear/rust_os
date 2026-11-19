//! The initial exception frame of a new thread (ARMv7-M). Pure data layout,
//! no hardware access.

// Initial exception frame of a new thread (top of its stack):
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

/// Words in a new thread's initial frame: 8 stacked by hardware on exception
/// entry (r0-r3, r12, LR, PC, xPSR) and 8 (r4-r11) restored by PendSV.
const INITIAL_FRAME_WORDS: usize = 16;

/// Write the frame a new thread starts from at the top of `stack`, which the
/// caller has zeroed (so the fabricated r0-r12 start at 0). Returns the index
/// of the word the thread's saved SP must point at.
pub(crate) fn init_stack(stack: &mut [u32], entry: usize) -> usize {
    let top = stack.len();
    // Thread entries are `-> !`, so this LR is never used. Poison it so a
    // bad return faults loudly instead of running garbage.
    stack[top - 3] = 0xFFFF_FFFF; // LR
    stack[top - 2] = (entry as u32) | 1; // PC, Thumb bit
    stack[top - 1] = 1 << 24; // xPSR: Thumb state
    top - INITIAL_FRAME_WORDS
}
