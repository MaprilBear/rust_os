///////////////////////////////////////////////////////////////////////////////////////////////////
// UART register definitions
///////////////////////////////////////////////////////////////////////////////////////////////////

struct Ro;
struct Wo;
struct Rw;
trait Readable {}
trait Writable {}
impl Readable for Ro {}
impl Readable for Rw {}
impl Writable for Wo {}
impl Writable for Rw {}

#[repr(transparent)]
struct Reg<T: Copy, A>(UnsafeCell<T>, PhantomData<A>);
impl<T: Copy, A: Readable> Reg<T, A> {
    fn read(&self) -> T {
        unsafe { ptr::read_volatile(self.0.get()) }
    }
}
impl<T: Copy, A: Writable> Reg<T, A> {
    fn write(&self, v: T) {
        unsafe { ptr::write_volatile(self.0.get(), v) }
    }
}

struct Task(Reg<u32, Wo>);
impl Task {
    fn trigger(&self) {
        self.0.write(1)
    }
}

struct Event(Reg<u32, Rw>);
impl Event {
    fn is_set(&self) -> bool {
        self.0.read() & 1 != 0
    }
    fn clear(&self) {
        self.0.write(0)
    }
}

#[bitfield(u32)]
struct UartShorts {
    #[bits(5)]
    __: u32,
    #[bits(1)]
    endrx_startrx: bool,
    #[bits(1)]
    endrx_stoprx: bool,
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
    disconnected: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
struct UartBaudRate(u32);
impl UartBaudRate {
    const B1200: Self = Self(0x0004_F000);
    const B2400: Self = Self(0x0009_D000);
    const B4800: Self = Self(0x0013_B000);
    const B9600: Self = Self(0x0027_5000);
    const B14400: Self = Self(0x003A_F000);
    const B19200: Self = Self(0x004E_A000);
    const B28800: Self = Self(0x0075_C000);
    const B31250: Self = Self(0x0080_0000);
    const B38400: Self = Self(0x009D_0000);
    const B56000: Self = Self(0x00E5_0000);
    const B57600: Self = Self(0x00EB_0000);
    const B76800: Self = Self(0x013A_9000);
    const B115200: Self = Self(0x01D6_0000);
    const B230400: Self = Self(0x03B0_0000);
    const B250000: Self = Self(0x0400_0000);
    const B460800: Self = Self(0x0740_0000);
    const B921600: Self = Self(0x0F00_0000);
    const B1M: Self = Self(0x1000_0000);
}

#[bitfield(u32)]
struct Count16 {
    #[bits(16)]
    count: u16,
    #[bits(16)]
    __: u32,
}

#[repr(C)]
struct Dma {
    ptr: Reg<u32, Rw>,
    maxcnt: Reg<Count16, Rw>,
    amount: Reg<Count16, Ro>,
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
    start_rx: Task,        // 0x000
    stop_rx: Task,         // 0x004
    start_tx: Task,        // 0x008
    stop_tx: Task,         // 0x00C
    _reserved0: [u32; 7],  // 0x010..0x02C
    flush_rx: Task,        // 0x02C
    _reserved1: [u32; 52], // 0x030..0x100

    cts: Event,            // 0x100
    ncts: Event,           // 0x104
    rxdrdy: Event,         // 0x108
    _reserved2a: [u32; 1], // 0x10C
    endrx: Event,          // 0x110
    _reserved2b: [u32; 2], // 0x114..0x11C
    txdrdy: Event,         // 0x11C
    endtx: Event,          // 0x120
    error: Event,          // 0x124
    _reserved3: [u32; 7],  // 0x128..0x144
    rxto: Event,           // 0x144
    _reserved4: [u32; 1],  // 0x148
    rxstarted: Event,      // 0x14C
    txstarted: Event,      // 0x150
    _reserved5: [u32; 1],  // 0x154
    txstopped: Event,      // 0x158
    _reserved6: [u32; 41], // 0x15C..0x200

    shorts: Reg<UartShorts, Rw>,      // 0x200
    _reserved7: [u32; 63],            // 0x204..0x300
    inten: Reg<UartInterrupt, Rw>,    // 0x300
    intenset: Reg<UartInterrupt, Rw>, // 0x304
    intenclr: Reg<UartInterrupt, Rw>, // 0x308
    _reserved8: [u32; 93],            // 0x30C..0x480
    errorsrc: Reg<UartErrorSrc, Rw>,  // 0x480
    _reserved9: [u32; 31],            // 0x484..0x500
    enable: Reg<UartEnable, Rw>,      // 0x500
    _reserved10: [u32; 1],            // 0x504

    psel_rts: Reg<UartPsel, Rw>, // 0x508
    psel_cts: Reg<UartPsel, Rw>, // 0x50C
    psel_txd: Reg<UartPsel, Rw>, // 0x510
    psel_rxd: Reg<UartPsel, Rw>, // 0x514
    _reserved11: [u32; 3],       // 0x518..0x524

    baudrate: Reg<UartBaudRate, Rw>, // 0x524
    _reserved12: [u32; 3],           // 0x528..0x534
    rxd: Dma,                        // 0x534..0x53C
    _reserved13: [u32; 1],           // 0x540
    txd: Dma,                        // 0x544..0x54C
    _reserved14: [u32; 7],           // 0x550..0x56C

    config: Reg<UartConfig, Rw>, // 0x56C
}

const UART0E_BASE: *mut UartRegs = 0x40002000 as *mut UartRegs;
const UART1E_BASE: *mut UartRegs = 0x40028000 as *mut UartRegs;

///////////////////////////////////////////////////////////////////////////////////////////////////
// UART Implementation
///////////////////////////////////////////////////////////////////////////////////////////////////

// The handle can move between contexts; only one exists per peripheral.
unsafe impl Send for Uarte {}

// UART peripherals are thread safe via semaphores
unsafe impl Sync for Uarte {}

pub struct Uarte {
    semaphore: UnsafeCell<Semaphore>,
    regs: *mut UartRegs,
}

pub static UARTE0: Uarte = Uarte {
    semaphore: UnsafeCell::new(Semaphore::with_count(1)),
    // It is safe to convert the pointer to a shared reference
    regs: UART0E_BASE,
};

pub static UARTE1: Uarte = Uarte {
    semaphore: UnsafeCell::new(Semaphore::with_count(1)),
    regs: UART1E_BASE,
};


impl Uarte {
    pub fn send(&self, buffer: &[u8]) {
        unsafe { (*self.semaphore.get()).take() };

        let regs = unsafe { &*self.regs };
        // Setup EasyDMA registers
        regs.txd.ptr.write(buffer.as_ptr() as u32);
        regs.txd.maxcnt.write(
            Count16Builder::new()
                .with_count(buffer.len().saturating_cast::<u16>())
                .build(),
        );

        // Trigger the transmission and busy wait
        regs.start_tx.trigger();
        // Busy wait until transmission is complete
        while !regs.endtx.is_set() {}
        regs.endtx.clear();

        unsafe { (*self.semaphore.get()).give() };
    }
}

use core::{cell::UnsafeCell, marker::PhantomData, ptr};

use bitfields::{bitfield, bitflag};
use saturating_cast::SaturatingCast;

use crate::Semaphore;

pub fn uart_init() {
    dprintln!("{:p}", UARTE0.regs);
    // Setup a basic non conflicting PSEL config (no RTS/CTS)
    // TODO: Allow this to be user defined
    let psel_rts = UartPselBuilder::new().with_disconnected(true).build();
    let psel_cts = UartPselBuilder::new().with_disconnected(true).build();
    let psel_txd = UartPselBuilder::new()
        .with_disconnected(false)
        .with_pin_num(0)
        .with_port_num(0)
        .build();
    let psel_rxd = UartPselBuilder::new()
        .with_disconnected(false)
        .with_pin_num(1)
        .with_port_num(0)
        .build();
    let regs = unsafe { &*UARTE0.regs };
    regs.psel_cts.write(psel_cts);
    regs.psel_rts.write(psel_rts);
    regs.psel_txd.write(psel_txd);
    regs.psel_rxd.write(psel_rxd);

    regs.baudrate.write(UartBaudRate::B115200);

    let config = UartConfigBuilder::new()
        .with_hardware_flow_control(false)
        .with_parity(UartConfigParityFlag::None)
        .with_stop_bits(UartConfigStopBitsFlag::One)
        .build();
    regs.config.write(config);

    let uart_enable = UartEnableBuilder::new().with_enable(true).build();
    regs.enable.write(uart_enable);
}

