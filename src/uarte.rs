///////////////////////////////////////////////////////////////////////////////////////////////////
// UART register definitions
///////////////////////////////////////////////////////////////////////////////////////////////////

/// Volatile register cell.
#[repr(transparent)]
struct Reg<T: Copy>(UnsafeCell<T>);

impl<T: Copy> Reg<T> {
    #[inline(always)]
    fn read(&self) -> T {
        unsafe { ptr::read_volatile(self.0.get()) }
    }
    #[inline(always)]
    fn write(&self, v: T) {
        unsafe { ptr::write_volatile(self.0.get(), v) }
    }
}

#[bitfield(u32)]
struct SingleBitWriteOnly {
    #[bits(1, access = wo)]
    flag: bool,
    #[bits(31)]
    __: u32,
}

#[bitfield(u32)]
struct SingleBit {
    #[bits(1)]
    flag: bool,
    #[bits(31)]
    __: u32,
}

#[bitfield(u32)]
struct UartShorts {
    #[bits(5)]
    __: u32,
    #[bits(1)]
    endrx_stoprx: bool,
    #[bits(1)]
    endrx_startrx: bool,
    #[bits(25)]
    __: u32,
}

#[bitfield(u32)]
struct UartInterrupt {
    #[bits(1)]
    cts: bool,
    #[bits(1)]
    ncts: bool,
    #[bits(1)]
    rxdrdy: bool,
    #[bits(1)]
    __: u32,
    #[bits(1)]
    endrx: bool,
    #[bits(2)]
    __: u32,
    #[bits(1)]
    txdrdy: bool,
    #[bits(1)]
    endtx: bool,
    #[bits(1)]
    error: bool,
    #[bits(7)]
    __: u32,
    #[bits(1)]
    rxto: bool,
    #[bits(1)]
    __: u32,
    #[bits(1)]
    rx_started: bool,
    #[bits(1)]
    tx_started: bool,
    #[bits(1)]
    __: u32,
    #[bits(1)]
    tx_stopped: bool,
    #[bits(9)]
    __: u32,
}

#[bitfield(u32)]
struct UartErrorSrc {
    #[bits(1)]
    overrun: bool,
    #[bits(1)]
    parity: bool,
    #[bits(1)]
    framing: bool,
    #[bits(1)]
    r#break: bool,
    #[bits(28)]
    __: u32,
}

#[bitfield(u32)]
struct UartEnable {
    #[bits(3)]
    __: u32,
    #[bits(1)]
    enable: bool,
    #[bits(28)]
    __: u32,
}

#[bitfield(u32)]
struct UartPsel {
    #[bits(5)]
    pin_num: u8,
    #[bits(1)]
    port_num: u8,
    #[bits(25)]
    __: u32,
    #[bits(1)]
    connect: bool,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UartBaudRate {
    Baud1200 = 0x0004_F000,
    Baud2400 = 0x0009_D000,
    Baud4800 = 0x0013_B000,
    Baud9600 = 0x0027_5000,
    Baud14400 = 0x003A_F000,
    Baud19200 = 0x004E_A000,
    Baud28800 = 0x0075_C000,
    Baud31250 = 0x0080_0000,
    Baud38400 = 0x009D_0000,
    Baud56000 = 0x00E5_0000,
    Baud57600 = 0x00EB_0000,
    Baud76800 = 0x013A_9000,
    Baud115200 = 0x01D6_0000,
    Baud230400 = 0x03B0_0000,
    Baud250000 = 0x0400_0000,
    Baud460800 = 0x0740_0000,
    Baud921600 = 0x0F00_0000,
    Baud1M = 0x1000_0000,
}

#[bitfield(u32)]
struct TwoByte {
    #[bits(16)]
    value: u16,
    #[bits(16)]
    __: u32,
}

#[bitflag(u8)]
#[derive(Debug, PartialEq)]
enum UartConfigParityFlag {
    #[base]
    None = 0x0,
    Even = 0x7,
}

#[bitflag(u8)]
#[derive(Debug, PartialEq)]
enum UartConfigStopBitsFlag {
    #[base]
    One = 0x0,
    Two = 0x1,
}

#[bitfield(u32)]
struct UartConfig {
    #[bits(1)]
    hardware_flow_control: bool,
    #[bits(3)]
    parity: UartConfigParityFlag,
    #[bits(1)]
    stop_bits: UartConfigStopBitsFlag,
    #[bits(27)]
    __: u32,
}

#[repr(C)]
struct UartRegs {
    start_rx: Reg<SingleBitWriteOnly>, // 0x000
    stop_rx: Reg<SingleBitWriteOnly>,  // 0x004
    start_tx: Reg<SingleBitWriteOnly>, // 0x008
    stop_tx: Reg<SingleBitWriteOnly>,  // 0x00C
    _reserved0: [u32; 7],              // 0x010..0x02C
    flush_rx: Reg<SingleBitWriteOnly>, // 0x02C
    _reserved1: [u32; 52],             // 0x030..0x100

    cts: Reg<SingleBit>,       // 0x100
    ncts: Reg<SingleBit>,      // 0x104
    rxdrdy: Reg<SingleBit>,    // 0x108
    _reserved2a: [u32; 1],     // 0x10C
    endrx: Reg<SingleBit>,     // 0x110
    _reserved2b: [u32; 2],     // 0x114..0x11C
    txdrdy: Reg<SingleBit>,    // 0x11C
    endtx: Reg<SingleBit>,     // 0x120
    error: Reg<SingleBit>,     // 0x124
    _reserved3: [u32; 7],      // 0x128..0x144
    rxto: Reg<SingleBit>,      // 0x144
    _reserved4: [u32; 1],      // 0x148
    rxstarted: Reg<SingleBit>, // 0x14C
    txstarted: Reg<SingleBit>, // 0x150
    _reserved5: [u32; 1],      // 0x154
    txstopped: Reg<SingleBit>, // 0x158
    _reserved6: [u32; 41],     // 0x15C..0x200

    shorts: Reg<UartShorts>,      // 0x200
    _reserved7: [u32; 63],        // 0x204..0x300
    inten: Reg<UartInterrupt>,    // 0x300
    intenset: Reg<UartInterrupt>, // 0x304
    intenclr: Reg<UartInterrupt>, // 0x308
    _reserved8: [u32; 93],        // 0x30C..0x480
    errorsrc: Reg<UartErrorSrc>,  // 0x480
    _reserved9: [u32; 31],        // 0x484..0x500
    enable: Reg<UartEnable>,      // 0x500
    _reserved10: [u32; 1],        // 0x504

    psel_rts: Reg<UartPsel>, // 0x508
    psel_cts: Reg<UartPsel>, // 0x50C
    psel_txd: Reg<UartPsel>, // 0x510
    psel_rxd: Reg<UartPsel>, // 0x514
    _reserved11: [u32; 3],   // 0x518..0x524

    baudrate: Reg<UartBaudRate>, // 0x524
    _reserved12: [u32; 3],       // 0x528..0x534
    rxd_ptr: Reg<u32>,           // 0x534
    rxd_maxcnt: Reg<TwoByte>,    // 0x538
    rxd_amount: Reg<TwoByte>,    // 0x53C
    _reserved13: [u32; 1],       // 0x540
    txd_ptr: Reg<u32>,           // 0x544
    txd_maxcnt: Reg<TwoByte>,    // 0x548
    txd_amount: Reg<TwoByte>,    // 0x54C
    _reserved14: [u32; 7],       // 0x550..0x56C

    config: Reg<UartConfig>, // 0x56C
}

const _: () = {
    assert!(offset_of!(UartRegs, endrx) == 0x110);
    assert!(offset_of!(UartRegs, txdrdy) == 0x11C);
    assert!(offset_of!(UartRegs, rxto) == 0x144);
    assert!(offset_of!(UartRegs, txstopped) == 0x158);
    assert!(offset_of!(UartRegs, shorts) == 0x200);
    assert!(offset_of!(UartRegs, inten) == 0x300);
    assert!(offset_of!(UartRegs, errorsrc) == 0x480);
    assert!(offset_of!(UartRegs, enable) == 0x500);
    assert!(offset_of!(UartRegs, baudrate) == 0x524);
    assert!(offset_of!(UartRegs, rxd_ptr) == 0x534);
    assert!(offset_of!(UartRegs, txd_ptr) == 0x544);
    assert!(offset_of!(UartRegs, config) == 0x56C);
};

const UART0E_BASE: u32 = 0x40002000;
const UART1E_BASE: u32 = 0x40028000;

///////////////////////////////////////////////////////////////////////////////////////////////////
// UART Implementation
///////////////////////////////////////////////////////////////////////////////////////////////////

pub struct Uart {
    regs: &'static UartRegs,
}

// The handle can move between contexts; only one exists per peripheral.
unsafe impl Send for Uart {}

static UART0_TAKEN: AtomicBool = AtomicBool::new(false);
static UART1_TAKEN: AtomicBool = AtomicBool::new(false);

impl Uart {
    fn claim(taken: &AtomicBool, base: usize) -> Option<Self> {
        if taken.swap(true, Ordering::AcqRel) {
            // The UART is already claimed, return None.
            None
        } else {
            // Runtime deref: fine. Unique because of the flag above.
            Some(Self {
                regs: unsafe { &*(base as *const UartRegs) },
            })
        }
    }

    pub fn take_uart0() -> Option<Self> {
        Self::claim(&UART0_TAKEN, 0x4000_2000)
    }
    pub fn give_uart0(self) {
        UART0_TAKEN.store(false, Ordering::Release);
    }
    pub fn take_uart1() -> Option<Self> {
        Self::claim(&UART1_TAKEN, 0x4002_8000)
    }
    pub fn give_uart1(self) {
        UART1_TAKEN.store(false, Ordering::Release);
    }
}

use core::{
    cell::UnsafeCell,
    mem::offset_of,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};

use bitfields::{bitfield, bitflag};
use saturating_cast::SaturatingCast;

pub fn uart_init() {
    let uart0 = Uart::take_uart0();
    if let Some(uart0) = uart0 {
        // Setup a basic non conflicting PSEL config (no RTS/CTS)
        // TODO: Allow this to be user defined
        // let psel_rts = UartPselBuilder::new().with_connect(false).build();
        // let psel_cts = UartPselBuilder::new().with_connect(false).build();
        // let psel_txd = UartPselBuilder::new()
        //     .with_connect(true)
        //     .with_pin_num(0)
        //     .with_port_num(0)
        //     .build();
        // let psel_rxd = UartPselBuilder::new()
        //     .with_connect(true)
        //     .with_pin_num(1)
        //     .with_port_num(0)
        //     .build();
        // uart0.regs.psel_cts.write(psel_cts);
        // uart0.regs.psel_rts.write(psel_rts);
        // uart0.regs.psel_txd.write(psel_txd);
        // uart0.regs.psel_rxd.write(psel_rxd);

        uart0.regs.baudrate.write(UartBaudRate::Baud57600);

        let config = UartConfigBuilder::new()
            .with_hardware_flow_control(false)
            .with_parity(UartConfigParityFlag::None)
            .with_stop_bits(UartConfigStopBitsFlag::One)
            .build();
        uart0.regs.config.write(config);

        let uart_enable = UartEnableBuilder::new().with_enable(true).build();
        uart0.regs.enable.write(uart_enable);

        uart0.give_uart0();
    }
}

pub fn uart_send(buffer: &[u8]) {
    let uart0 = Uart::take_uart0();
    if let Some(uart0) = uart0 {
        // Setup EasyDMA registers
        uart0.regs.txd_ptr.write(buffer.as_ptr() as u32);
        uart0.regs.txd_maxcnt.write(
            TwoByteBuilder::new()
                .with_value(buffer.len().saturating_cast::<u16>())
                .build(),
        );

        // Trigger the transmission and busy wait
        uart0
            .regs
            .start_tx
            .write(SingleBitWriteOnlyBuilder::new().with_flag(true).build());
        // Busy wait until transmission is complete
        while !uart0.regs.endtx.read().flag() {}
        uart0.give_uart0();
    }
}
