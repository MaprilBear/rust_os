///////////////////////////////////////////////////////////////////////////////////////////////////
// UART register definitions
///////////////////////////////////////////////////////////////////////////////////////////////////

use crate::fifo::{Fifo, LockFreePolicy};
use crate::mmio::{Count16, Count16Builder, Event, Mmio, Reg, Ro, Rw, Task};
use kernel::semaphore;
use userlib::Global;

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

#[allow(unused)]
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
struct UarteRegs {
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

const UART0E_BASE: usize = 0x40002000;
#[allow(unused)]
const UART1E_BASE: usize = 0x40028000;

///////////////////////////////////////////////////////////////////////////////////////////////////
// UART Implementation
///////////////////////////////////////////////////////////////////////////////////////////////////

// The handle can move between contexts; only one exists per peripheral.
unsafe impl Send for Uarte {}

// UART peripherals are thread safe via semaphores
unsafe impl Sync for Uarte {}

pub struct Uarte {
    regs_semaphore: Semaphore,
    tx_semaphore: Semaphore,
    rx_fifo: Fifo<u8, 256, LockFreePolicy>,
    rx_buffer_1: Global<[u8; 1]>,
    rx_buffer_2: Global<[u8; 1]>,
    regs: Mmio<UarteRegs>,
}

pub static UARTE0: Uarte = unsafe { Uarte::new(UART0E_BASE) };
#[allow(unused)]
pub static UARTE1: Uarte = unsafe { Uarte::new(UART1E_BASE) };

impl Uarte {
    const unsafe fn new(base_address: usize) -> Self {
        Self {
            regs_semaphore: semaphore!(1),
            tx_semaphore: semaphore!(0),
            rx_fifo: Fifo::new(LockFreePolicy::new()),
            rx_buffer_1: Global::new([0; 1]),
            rx_buffer_2: Global::new([0; 1]),
            regs: unsafe { Mmio::new(base_address) },
        }
    }

    pub fn send(&self, buffer: &[u8]) {
        self.regs_semaphore.take();

        let regs = &*self.regs;
        // Setup EasyDMA registers
        regs.txd.ptr.write(buffer.as_ptr() as u32);
        regs.txd.maxcnt.write(
            Count16Builder::new()
                .with_count(buffer.len().saturating_cast::<u16>())
                .build(),
        );

        // Start from a clean ENDTX, enable only its interrupt (INTENSET is write-1-to-enable, so
        // pass just the bit we want rather than a read-modify-write of INTEN), and begin the
        // transmission. The ENDTX interrupt handler will release the semaphore.
        regs.endtx.clear();
        regs.intenset
            .write(UartInterruptBuilder::new().with_endtx(true).build());
        regs.start_tx.trigger();
        self.tx_semaphore.take();
        self.regs_semaphore.give();
    }

    /// Read bytes into the provided buffer until full, or until the given timeout is exceeded.
    /// Returns the number of bytes received.
    pub fn receive(&self, buffer: &mut [u8]) -> u16 {
        self.rx_fifo.pop_into(buffer).saturating_cast::<u16>()
    }

    fn handle_interrupt(&self) {
        let regs = &*self.regs;

        if regs.endtx.generated() {
            regs.endtx.clear();
            // INTENCLR: writing 1 disables that interrupt, writing 0 does nothing. Pass only
            // the bit(s) being disabled.
            regs.intenclr
                .write(UartInterruptBuilder::new().with_endtx(true).build());
            kernel::give_from_isr(&self.tx_semaphore);
        }

        // Swap the receive buffers as the other fills up and push the received byte into the FIFO.
        if regs.endrx.generated() {
            regs.endrx.clear();
            let current = regs.rxd.ptr.read() as *const u8;
            let (done, next) = if current == self.rx_buffer_1.as_ptr() as *const u8 {
                (&self.rx_buffer_1, &self.rx_buffer_2)
            } else if current == self.rx_buffer_2.as_ptr() as *const u8 {
                (&self.rx_buffer_2, &self.rx_buffer_1)
            } else {
                panic!("Unexpected RX buffer pointer: {:p}", current);
            };
            let _ = self.rx_fifo.push(done.read()[0]);
            regs.rxd.ptr.write(next.as_ptr() as u32);
            regs.start_rx.trigger(); // TODO: remove on real hardware
        }
    }
}

pub extern "C" fn uarte0_handler() {
    // Acknowledge the interrupt
    let core_periph = core_periph();
    unsafe { core_periph.NVIC.icpr[0].write(1 << Interrupt::UARTE0_UART0.number()) };

    // Let the UARTE peripheral handle the interrupt
    UARTE0.handle_interrupt();
}

use bitfields::{bitfield, bitflag};
use cortex_m::interrupt::InterruptNumber;
use saturating_cast::SaturatingCast;

use kernel::{core_periph, Interrupt};
use userlib::Semaphore;

/// # Safety
/// This function should not be called post-OS initialization.
pub fn uart_init() {
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
    let regs = &*UARTE0.regs;
    regs.psel_cts.write(psel_cts);
    regs.psel_rts.write(psel_rts);
    regs.psel_txd.write(psel_txd);
    regs.psel_rxd.write(psel_rxd);

    regs.baudrate.write(UartBaudRate::B115200);

    let config = UartConfigBuilder::new()
        .with_hardware_flow_control(true)
        .with_parity(UartConfigParityFlag::None)
        .with_stop_bits(UartConfigStopBitsFlag::One)
        .build();
    regs.config.write(config);

    let uart_enable = UartEnableBuilder::new().with_enable(true).build();
    regs.enable.write(uart_enable);

    let core_periph = core_periph();
    unsafe { core_periph.NVIC.set_priority(Interrupt::UARTE0_UART0, 0xC0) };
    // Enable interrupts for ENDRX to handle double buffer switching
    regs.intenset
        .write(UartInterruptBuilder::new().with_endrx(true).build());

    // enable the interrupt in NVIC
    unsafe { core_periph.NVIC.iser[0].modify(|w| w | (1 << Interrupt::UARTE0_UART0.number())) };
    // begin the background RX process
    regs.rxd.ptr.write(UARTE0.rx_buffer_1.as_ptr() as u32);
    regs.rxd
        .maxcnt
        .write(Count16Builder::new().with_count(1).build());
    regs.shorts
        .write(UartShortsBuilder::new().with_endrx_startrx(true).build());
    regs.start_rx.trigger();
}
