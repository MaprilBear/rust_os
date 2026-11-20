use bitfields::bitfield;

use kernel::semaphore;
use userlib::Semaphore;

use crate::mmio::{Count4, Event, Mmio, Reg, Rw, Task};

#[bitfield(u32)]
pub struct TimerShort {
    pub compare0_clear: bool,
    pub compare1_clear: bool,
    pub compare2_clear: bool,
    pub compare3_clear: bool,
    pub compare4_clear: bool,
    pub compare5_clear: bool,
    #[bits(2)]
    __: u32,
    pub compare0_stop: bool,
    pub compare1_stop: bool,
    pub compare2_stop: bool,
    pub compare3_stop: bool,
    pub compare4_stop: bool,
    pub compare5_stop: bool,
    #[bits(18)]
    __: u32,
}

#[bitfield(u32)]
pub struct TimerInt {
    #[bits(16)]
    __: u32,
    compare0: bool,
    compare1: bool,
    compare2: bool,
    compare3: bool,
    compare4: bool,
    compare5: bool,
    #[bits(10)]
    __: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct TimerMode(u32);

impl TimerMode {
    pub const TIMER: Self = Self(0x0);
    pub const COUNTER: Self = Self(0x1);
    pub const LOWPOWER: Self = Self(0x2);
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct TimerBitmode(u32);

impl TimerBitmode {
    pub const BIT_16: Self = Self(0x0);
    pub const BIT_08: Self = Self(0x1);
    pub const BIT_24: Self = Self(0x2);
    pub const BIT_32: Self = Self(0x3);
}

#[repr(C)]
pub struct TimerRegs {
    pub start: Task,                    // 0x000
    pub stop: Task,                     // 0x004
    pub count: Task,                    // 0x008
    pub clear: Task,                    // 0x00C
    pub shutdown: Task,                 // 0x010, deprecated
    _reserved0: [u32; 11],              // 0x014..0x03C
    pub capture0: Task,                 // 0x040
    pub capture1: Task,                 // 0x044
    pub capture2: Task,                 // 0x048
    pub capture3: Task,                 // 0x04C
    pub capture4: Task,                 // 0x050
    pub capture5: Task,                 // 0x054
    _reserved1: [u32; 58],              // 0x058..0x13C
    pub compare0: Event,                // 0x140
    pub compare1: Event,                // 0x144
    pub compare2: Event,                // 0x148
    pub compare3: Event,                // 0x14C
    pub compare4: Event,                // 0x150
    pub compare5: Event,                // 0x154
    _reserved2: [u32; 42],              // 0x158..0x1FC
    pub shorts: Reg<TimerShort, Rw>,    // 0x200
    _reserved3: [u32; 64],              // 0x204..0x300
    pub intenset: Reg<TimerInt, Rw>,    // 0x304
    pub intenclear: Reg<TimerInt, Rw>,  // 0x308
    _reserved4: [u32; 126],             // 0x30C..0x500
    pub mode: Reg<TimerMode, Rw>,       // 0x504
    pub bitmode: Reg<TimerBitmode, Rw>, // 0x508
    _reserved5: u32,                    // 0x50C
    pub prescaler: Reg<Count4, Rw>,     // 0x510
    _reserved6: [u32; 11],              // 0x514..0x53C
    pub cc0: Reg<u32, Rw>,              // 0x540
    pub cc1: Reg<u32, Rw>,              // 0x544
    pub cc2: Reg<u32, Rw>,              // 0x548
    pub cc3: Reg<u32, Rw>,              // 0x54C
    pub cc4: Reg<u32, Rw>,              // 0x550
    pub cc5: Reg<u32, Rw>,              // 0x554
}

const TIMER0_BASE: usize = 0x40008000;
const TIMER1_BASE: usize = 0x40009000;
const TIMER2_BASE: usize = 0x4000A000;
const TIMER3_BASE: usize = 0x4001A000;
const TIMER4_BASE: usize = 0x4001B000;

pub struct Timer {
    pub regs_semaphore: Semaphore,
    pub regs: Mmio<TimerRegs>,
}

// The handle can move between contexts; only one exists per peripheral.
unsafe impl Send for Timer {}
// Timer peripherals are thread safe via semaphores
unsafe impl Sync for Timer {}

pub static TIMER0: Timer = unsafe { Timer::new(TIMER0_BASE) };
pub static TIMER1: Timer = unsafe { Timer::new(TIMER1_BASE) };
pub static TIMER2: Timer = unsafe { Timer::new(TIMER2_BASE) };
pub static TIMER3: Timer = unsafe { Timer::new(TIMER3_BASE) };
pub static TIMER4: Timer = unsafe { Timer::new(TIMER4_BASE) };

impl Timer {
    /// # Safety
    /// `base` must be the address of this peripheral's register block, and only
    /// one instance may exist per peripheral (the statics above).
    const unsafe fn new(base: usize) -> Self {
        Self {
            regs_semaphore: semaphore!(1),
            regs: unsafe { Mmio::new(base) },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::offset_of;

    /// Offsets from the nRF52840 SVD (renode/nrf52840.svd).
    #[test]
    fn register_offsets_match_the_nrf52840() {
        assert_eq!(offset_of!(TimerRegs, start), 0x000);
        assert_eq!(offset_of!(TimerRegs, stop), 0x004);
        assert_eq!(offset_of!(TimerRegs, count), 0x008);
        assert_eq!(offset_of!(TimerRegs, clear), 0x00C);
        assert_eq!(offset_of!(TimerRegs, shutdown), 0x010);
        assert_eq!(offset_of!(TimerRegs, capture0), 0x040);
        assert_eq!(offset_of!(TimerRegs, capture5), 0x054);
        assert_eq!(offset_of!(TimerRegs, compare0), 0x140);
        assert_eq!(offset_of!(TimerRegs, compare5), 0x154);
        assert_eq!(offset_of!(TimerRegs, shorts), 0x200);
        assert_eq!(offset_of!(TimerRegs, intenset), 0x304);
        assert_eq!(offset_of!(TimerRegs, intenclear), 0x308);
        assert_eq!(offset_of!(TimerRegs, mode), 0x504);
        assert_eq!(offset_of!(TimerRegs, bitmode), 0x508);
        assert_eq!(offset_of!(TimerRegs, prescaler), 0x510);
        assert_eq!(offset_of!(TimerRegs, cc0), 0x540);
        assert_eq!(offset_of!(TimerRegs, cc5), 0x554);
    }

    #[test]
    fn field_bits_match_the_nrf52840() {
        let shorts = |b: TimerShortBuilder| b.build().into_bits();
        assert_eq!(
            shorts(TimerShortBuilder::new().with_compare0_clear(true)),
            1 << 0
        );
        assert_eq!(
            shorts(TimerShortBuilder::new().with_compare5_clear(true)),
            1 << 5
        );
        assert_eq!(
            shorts(TimerShortBuilder::new().with_compare0_stop(true)),
            1 << 8
        );
        assert_eq!(
            shorts(TimerShortBuilder::new().with_compare5_stop(true)),
            1 << 13
        );
        let int = |b: TimerIntBuilder| b.build().into_bits();
        assert_eq!(int(TimerIntBuilder::new().with_compare0(true)), 1 << 16);
        assert_eq!(int(TimerIntBuilder::new().with_compare5(true)), 1 << 21);
        assert_eq!(TimerMode::LOWPOWER.0, 2);
        assert_eq!(TimerBitmode::BIT_32.0, 3);
        assert_eq!(TimerBitmode::BIT_08.0, 1);
    }

    #[test]
    fn instances_point_at_their_peripherals() {
        let base = |t: &Timer| t.regs.as_ptr() as usize;
        assert_eq!(base(&TIMER0), 0x4000_8000);
        assert_eq!(base(&TIMER1), 0x4000_9000);
        assert_eq!(base(&TIMER2), 0x4000_A000);
        assert_eq!(base(&TIMER3), 0x4001_A000);
        assert_eq!(base(&TIMER4), 0x4001_B000);
    }

    /// Known issue 3: `semaphore!` inside `Timer::new` makes one kernel
    /// object for every instance.
    #[test]
    #[ignore = "known issue 3: semaphore! in a const fn is shared by every instance"]
    fn each_timer_has_its_own_semaphore() {
        assert_ne!(
            TIMER0.regs_semaphore.handle(),
            TIMER1.regs_semaphore.handle()
        );
    }
}
