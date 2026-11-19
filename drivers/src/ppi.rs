use bitfields::bitfield;

use kernel::semaphore;
use userlib::Semaphore;

use crate::mmio::{Mmio, Reg, Rw, Task};

#[bitfield(u32)]
pub struct ChannelReg {
    ch0: bool,
    ch1: bool,
    ch2: bool,
    ch3: bool,
    ch4: bool,
    ch5: bool,
    ch6: bool,
    ch7: bool,
    ch8: bool,
    ch9: bool,
    ch10: bool,
    ch11: bool,
    ch12: bool,
    ch13: bool,
    ch14: bool,
    ch15: bool,
    ch16: bool,
    ch17: bool,
    ch18: bool,
    ch19: bool,
    ch20: bool,
    ch21: bool,
    ch22: bool,
    ch23: bool,
    ch24: bool,
    ch25: bool,
    ch26: bool,
    ch27: bool,
    ch28: bool,
    ch29: bool,
    ch30: bool,
    ch31: bool,
}

#[repr(C)]
pub struct PpiReg {
    pub chg0_en: Task,                  // 0x000
    pub chg0_dis: Task,                 // 0x004
    pub chg1_en: Task,                  // 0x008
    pub chg1_dis: Task,                 // 0x00C
    pub chg2_en: Task,                  // 0x010
    pub chg2_dis: Task,                 // 0x014
    pub chg3_en: Task,                  // 0x018
    pub chg3_dis: Task,                 // 0x01C
    pub chg4_en: Task,                  // 0x020
    pub chg4_dis: Task,                 // 0x024
    pub chg5_en: Task,                  // 0x028
    pub chg5_dis: Task,                 // 0x02C
    _reserved0: [u32; 308],             // 0x030..0x4FC
    pub chen: Reg<ChannelReg, Rw>,      // 0x500
    pub chenset: Reg<ChannelReg, Rw>,   // 0x504
    pub chenclear: Reg<ChannelReg, Rw>, // 0x508
    _reserved1: u32,                    // 0x50C
    pub ch0_eep: Reg<u32, Rw>,          // 0x510
    pub ch0_tep: Reg<u32, Rw>,          // 0x514
    pub ch1_eep: Reg<u32, Rw>,          // 0x518
    pub ch1_tep: Reg<u32, Rw>,          // 0x51C
    pub ch2_eep: Reg<u32, Rw>,          // 0x520
    pub ch2_tep: Reg<u32, Rw>,          // 0x524
    pub ch3_eep: Reg<u32, Rw>,          // 0x528
    pub ch3_tep: Reg<u32, Rw>,          // 0x52C
    pub ch4_eep: Reg<u32, Rw>,          // 0x530
    pub ch4_tep: Reg<u32, Rw>,          // 0x534
    pub ch5_eep: Reg<u32, Rw>,          // 0x538
    pub ch5_tep: Reg<u32, Rw>,          // 0x53C
    pub ch6_eep: Reg<u32, Rw>,          // 0x540
    pub ch6_tep: Reg<u32, Rw>,          // 0x544
    pub ch7_eep: Reg<u32, Rw>,          // 0x548
    pub ch7_tep: Reg<u32, Rw>,          // 0x54C
    pub ch8_eep: Reg<u32, Rw>,          // 0x550
    pub ch8_tep: Reg<u32, Rw>,          // 0x554
    pub ch9_eep: Reg<u32, Rw>,          // 0x558
    pub ch9_tep: Reg<u32, Rw>,          // 0x55C
    pub ch10_eep: Reg<u32, Rw>,         // 0x560
    pub ch10_tep: Reg<u32, Rw>,         // 0x564
    pub ch11_eep: Reg<u32, Rw>,         // 0x568
    pub ch11_tep: Reg<u32, Rw>,         // 0x56C
    pub ch12_eep: Reg<u32, Rw>,         // 0x570
    pub ch12_tep: Reg<u32, Rw>,         // 0x574
    pub ch13_eep: Reg<u32, Rw>,         // 0x578
    pub ch13_tep: Reg<u32, Rw>,         // 0x57C
    pub ch14_eep: Reg<u32, Rw>,         // 0x580
    pub ch14_tep: Reg<u32, Rw>,         // 0x584
    pub ch15_eep: Reg<u32, Rw>,         // 0x588
    pub ch15_tep: Reg<u32, Rw>,         // 0x58C
    pub ch16_eep: Reg<u32, Rw>,         // 0x590
    pub ch16_tep: Reg<u32, Rw>,         // 0x594
    pub ch17_eep: Reg<u32, Rw>,         // 0x598
    pub ch17_tep: Reg<u32, Rw>,         // 0x59C
    pub ch18_eep: Reg<u32, Rw>,         // 0x5A0
    pub ch18_tep: Reg<u32, Rw>,         // 0x5A4
    pub ch19_eep: Reg<u32, Rw>,         // 0x5A8
    pub ch19_tep: Reg<u32, Rw>,         // 0x5AC
    _reserved: [u32; 148],              // 0x5B0..0x7FC
    pub chg0: Reg<ChannelReg, Rw>,      // 0x800
    pub chg1: Reg<ChannelReg, Rw>,      // 0x804
    pub chg2: Reg<ChannelReg, Rw>,      // 0x808
    pub chg3: Reg<ChannelReg, Rw>,      // 0x80C
    pub chg4: Reg<ChannelReg, Rw>,      // 0x810
    pub chg5: Reg<ChannelReg, Rw>,      // 0x814
    _reserved2: [u32; 62],              // 0x818..0x90C
    pub fork0_tep: Reg<u32, Rw>,        // 0x910
    pub fork1_tep: Reg<u32, Rw>,        // 0x914
    pub fork2_tep: Reg<u32, Rw>,        // 0x918
    pub fork3_tep: Reg<u32, Rw>,        // 0x91C
    pub fork4_tep: Reg<u32, Rw>,        // 0x920
    pub fork5_tep: Reg<u32, Rw>,        // 0x924
    pub fork6_tep: Reg<u32, Rw>,        // 0x928
    pub fork7_tep: Reg<u32, Rw>,        // 0x92C
    pub fork8_tep: Reg<u32, Rw>,        // 0x930
    pub fork9_tep: Reg<u32, Rw>,        // 0x934
    pub fork10_tep: Reg<u32, Rw>,       // 0x938
    pub fork11_tep: Reg<u32, Rw>,       // 0x93C
    pub fork12_tep: Reg<u32, Rw>,       // 0x940
    pub fork13_tep: Reg<u32, Rw>,       // 0x944
    pub fork14_tep: Reg<u32, Rw>,       // 0x948
    pub fork15_tep: Reg<u32, Rw>,       // 0x94C
    pub fork16_tep: Reg<u32, Rw>,       // 0x950
    pub fork17_tep: Reg<u32, Rw>,       // 0x954
    pub fork18_tep: Reg<u32, Rw>,       // 0x958
    pub fork19_tep: Reg<u32, Rw>,       // 0x95C
    pub fork20_tep: Reg<u32, Rw>,       // 0x960
    pub fork21_tep: Reg<u32, Rw>,       // 0x964
    pub fork22_tep: Reg<u32, Rw>,       // 0x968
    pub fork23_tep: Reg<u32, Rw>,       // 0x96C
    pub fork24_tep: Reg<u32, Rw>,       // 0x970
    pub fork25_tep: Reg<u32, Rw>,       // 0x974
    pub fork26_tep: Reg<u32, Rw>,       // 0x978
    pub fork27_tep: Reg<u32, Rw>,       // 0x97C
    pub fork28_tep: Reg<u32, Rw>,       // 0x980
    pub fork29_tep: Reg<u32, Rw>,       // 0x984
    pub fork30_tep: Reg<u32, Rw>,       // 0x988
    pub fork31_tep: Reg<u32, Rw>,       // 0x98C
}

#[allow(unused)]
const PPI_BASE: usize = 0x4001F000;

#[allow(unused)]
pub struct Ppi {
    pub regs_semaphore: Semaphore,
    pub regs: Mmio<PpiReg>,
}

// The handle can move between contexts; only one exists per peripheral.
unsafe impl Send for Ppi {}
// The PPI peripheral is thread safe via semaphores
unsafe impl Sync for Ppi {}

#[allow(unused)]
pub static PPI: Ppi = unsafe { Ppi::new(PPI_BASE) };

#[allow(unused)]
impl Ppi {
    pub const unsafe fn new(base: usize) -> Self {
        Self {
            regs_semaphore: semaphore!(1),
            regs: unsafe { Mmio::new(base) },
        }
    }
}
