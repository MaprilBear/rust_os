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

#[allow(unused)]
impl TimerMode {
    pub const TIMER: Self = Self(0x0);
    pub const COUNTER: Self = Self(0x1);
    pub const LOWPOWER: Self = Self(0x2);
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct TimerBitmode(u32);

#[allow(unused)]
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

#[allow(unused)]
const TIMER0_BASE: usize = 0x40008000;
#[allow(unused)]
const TIMER1_BASE: usize = 0x40009000;
#[allow(unused)]
const TIMER2_BASE: usize = 0x4000A000;
#[allow(unused)]
const TIMER3_BASE: usize = 0x4001A000;
#[allow(unused)]
const TIMER4_BASE: usize = 0x4001B000;

#[allow(unused)]
pub struct Timer {
    pub regs_semaphore: Semaphore,
    pub regs: Mmio<TimerRegs>,
}

// The handle can move between contexts; only one exists per peripheral.
unsafe impl Send for Timer {}
// Timer peripherals are thread safe via semaphores
unsafe impl Sync for Timer {}

#[allow(unused)]
pub static TIMER0: Timer = unsafe { Timer::new(TIMER0_BASE) };
#[allow(unused)]
pub static TIMER1: Timer = unsafe { Timer::new(TIMER1_BASE) };
#[allow(unused)]
pub static TIMER2: Timer = unsafe { Timer::new(TIMER2_BASE) };
#[allow(unused)]
pub static TIMER3: Timer = unsafe { Timer::new(TIMER3_BASE) };
#[allow(unused)]
pub static TIMER4: Timer = unsafe { Timer::new(TIMER4_BASE) };

impl Timer {
    pub const unsafe fn new(base: usize) -> Self {
        Self {
            regs_semaphore: semaphore!(1),
            regs: unsafe { Mmio::new(base) },
        }
    }
}
