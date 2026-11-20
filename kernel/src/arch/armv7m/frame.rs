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

#[cfg(test)]
mod tests {
    use super::{init_stack, INITIAL_FRAME_WORDS};

    #[test]
    fn frame_sits_at_the_top_of_the_stack() {
        let mut stack = [0u32; 64];
        assert_eq!(init_stack(&mut stack, 0x1000), 64 - INITIAL_FRAME_WORDS);
    }

    #[test]
    fn frame_holds_entry_lr_poison_and_thumb_xpsr() {
        let mut stack = [0u32; 32];
        init_stack(&mut stack, 0x0001_2340);
        assert_eq!(stack[31], 1 << 24, "xPSR: only the Thumb bit");
        assert_eq!(stack[30], 0x0001_2341, "PC: entry with the Thumb bit");
        assert_eq!(stack[29], 0xFFFF_FFFF, "LR: poisoned");
    }

    #[test]
    fn entry_with_thumb_bit_already_set_is_unchanged() {
        let mut stack = [0u32; 32];
        init_stack(&mut stack, 0x0001_2341);
        assert_eq!(stack[30], 0x0001_2341);
    }

    #[test]
    fn registers_r0_to_r12_and_the_rest_of_the_stack_are_untouched() {
        let mut stack = [0xAAAA_AAAAu32; 40];
        let sp = init_stack(&mut stack, 0x1000);
        // r4-r11 (sp..sp+8), then r0-r3, r12 (sp+8..sp+13).
        assert!(stack[sp..sp + 13].iter().all(|&w| w == 0xAAAA_AAAA));
        assert!(stack[..sp].iter().all(|&w| w == 0xAAAA_AAAA));
    }
}

#[cfg(test)]
mod props {
    use super::{init_stack, INITIAL_FRAME_WORDS};
    use proptest::prelude::*;
    use std::vec;

    proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig {
            cases: if cfg!(miri) { 8 } else { 256 },
            failure_persistence: if cfg!(miri) {
                None
            } else {
                proptest::prelude::ProptestConfig::default().failure_persistence
            },
            ..proptest::prelude::ProptestConfig::default()
        })]

        #[test]
        fn frame_is_always_the_top_16_words(
            len in INITIAL_FRAME_WORDS..512,
            entry in any::<u32>(),
            fill in any::<u32>(),
        ) {
            let mut stack = vec![fill; len];
            let sp = init_stack(&mut stack, entry as usize);
            prop_assert_eq!(sp, len - INITIAL_FRAME_WORDS);
            prop_assert_eq!(stack[len - 2], entry | 1);
            prop_assert_eq!(stack[len - 1], 1 << 24);
            prop_assert_eq!(stack[len - 3], 0xFFFF_FFFF);
            prop_assert!(stack[..len - 3].iter().all(|&w| w == fill));
        }
    }
}
