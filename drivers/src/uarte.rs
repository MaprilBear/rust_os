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

#[allow(unused)] // the full table, though only some rates are used
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
pub static UARTE1: Uarte = unsafe { Uarte::new(UART1E_BASE) };

impl Uarte {
    const unsafe fn new(base_address: usize) -> Self {
        Self::with_regs(unsafe { Mmio::new(base_address) })
    }

    /// The instance over `regs` (host tests pass a fake block).
    const fn with_regs(regs: Mmio<UarteRegs>) -> Self {
        Self {
            regs_semaphore: semaphore!(1),
            tx_semaphore: semaphore!(0),
            rx_fifo: Fifo::new(LockFreePolicy::new()),
            rx_buffer_1: Global::new([0; 1]),
            rx_buffer_2: Global::new([0; 1]),
            regs,
        }
    }

    pub fn send(&self, buffer: &[u8]) {
        self.regs_semaphore.take();

        let regs = &*self.regs;
        // Setup EasyDMA registers
        regs.txd.ptr.write(buffer.as_ptr() as u32);
        regs.txd.maxcnt.write(
            Count16Builder::new()
                .with_count(SaturatingCast::saturating_cast::<u16>(buffer.len()))
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
        SaturatingCast::saturating_cast::<u16>(self.rx_fifo.pop_into(buffer))
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
            // Compare as the register holds them (32-bit), so this also runs
            // in host tests, where addresses are wider.
            let current = regs.rxd.ptr.read();
            let (done, next) = if current == self.rx_buffer_1.as_ptr() as u32 {
                (&self.rx_buffer_1, &self.rx_buffer_2)
            } else if current == self.rx_buffer_2.as_ptr() as u32 {
                (&self.rx_buffer_2, &self.rx_buffer_1)
            } else {
                panic!("Unexpected RX buffer pointer: {:#x}", current);
            };
            let _ = self.rx_fifo.push(done.read()[0]);
            regs.rxd.ptr.write(next.as_ptr() as u32);
            regs.start_rx.trigger(); // TODO: remove on real hardware
        }
    }
}

pub extern "C" fn uarte0_handler() {
    // Acknowledge the interrupt
    kernel::irq_unpend(Interrupt::UARTE0_UART0);

    // Let the UARTE peripheral handle the interrupt
    UARTE0.handle_interrupt();
}

use bitfields::{bitfield, bitflag};
use saturating_cast::SaturatingCast;

use kernel::Interrupt;
use userlib::Semaphore;

/// Configure UARTE0 and start background reception. Call once, from `user_main`
/// (privileged, before the scheduler starts), after attaching `uarte0_handler`
/// with `kernel::irq_attach`.
pub fn uart_init() {
    UARTE0.init(Interrupt::UARTE0_UART0);
}

impl Uarte {
    /// `uart_init` for this instance, whose interrupt line is `irq`. A method
    /// so host tests can run it on an instance over fake registers.
    fn init(&self, irq: Interrupt) {
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
        let regs = &*self.regs;
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

        kernel::irq_set_priority(irq, 0xC0);
        // Enable interrupts for ENDRX to handle double buffer switching
        regs.intenset
            .write(UartInterruptBuilder::new().with_endrx(true).build());

        // enable the interrupt in NVIC
        kernel::irq_enable(irq);
        // begin the background RX process
        regs.rxd.ptr.write(self.rx_buffer_1.as_ptr() as u32);
        regs.rxd
            .maxcnt
            .write(Count16Builder::new().with_count(1).build());
        regs.shorts
            .write(UartShortsBuilder::new().with_endrx_startrx(true).build());
        regs.start_rx.trigger();
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use core::mem::{offset_of, size_of};
    use kernel::test_support::*;
    use kernel::{ThreadId, ThreadState};
    use std::boxed::Box;
    use std::vec::Vec;

    // Register offsets from the nRF52840 SVD (renode/nrf52840.svd).
    const STARTRX: usize = 0x000;
    const STARTTX: usize = 0x008;
    const ENDRX: usize = 0x110;
    const ENDTX: usize = 0x120;
    const SHORTS: usize = 0x200;
    const INTENSET: usize = 0x304;
    const INTENCLR: usize = 0x308;
    const ENABLE: usize = 0x500;
    const BAUDRATE: usize = 0x524;
    const RXD_PTR: usize = 0x534;
    const RXD_MAXCNT: usize = 0x538;
    const TXD_PTR: usize = 0x544;
    const TXD_MAXCNT: usize = 0x548;
    const CONFIG: usize = 0x56C;

    const INT_ENDRX: u32 = 1 << 4;
    const INT_ENDTX: u32 = 1 << 8;

    /// Words in the fake register block: the whole `UarteRegs`.
    const WORDS: usize = size_of::<UarteRegs>() / 4;

    /// A `Uarte` over a zeroed register block in host memory.
    struct Fake {
        /// Raw (not a Box), so the driver's writes through its own pointer
        /// never conflict with a unique owner.
        mem: *mut [u32; WORDS],
        uarte: Box<Uarte>,
    }

    impl Fake {
        fn new() -> Self {
            let mem = Box::into_raw(Box::new([0u32; WORDS]));
            // Boxed: the RX handler compares RXD.PTR with the addresses of
            // the instance's own buffers, so it must not move.
            let regs = unsafe { Mmio::from_ptr(mem as *const UarteRegs) };
            let uarte = Box::new(Uarte::with_regs(regs));
            // Known issue 3: these are UARTE0's semaphores too.
            register_semaphore(&uarte.regs_semaphore);
            register_semaphore(&uarte.tx_semaphore);
            Fake { mem, uarte }
        }
        fn get(&self, offset: usize) -> u32 {
            unsafe { (self.mem as *const u32).add(offset / 4).read_volatile() }
        }
        fn set(&self, offset: usize, value: u32) {
            unsafe { (self.mem as *mut u32).add(offset / 4).write_volatile(value) }
        }
        fn interrupt(&self) {
            in_isr(|| self.uarte.handle_interrupt());
        }
        /// What the hardware does when a 1-byte RX transfer ends: the byte is
        /// in the buffer RXD.PTR points at, and ENDRX is set.
        fn receive_byte(&self, byte: u8) {
            let ptr = self.get(RXD_PTR);
            let buf = [&self.uarte.rx_buffer_1, &self.uarte.rx_buffer_2]
                .into_iter()
                .find(|b| b.as_ptr() as u32 == ptr)
                .expect("RXD.PTR is not one of the RX buffers");
            unsafe { (*buf.as_ptr())[0] = byte };
            self.set(ENDRX, 1);
        }
        fn received(&self) -> Vec<u8> {
            let mut out = [0u8; 300];
            let n = self.uarte.receive(&mut out) as usize;
            out[..n].to_vec()
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            drop(unsafe { Box::from_raw(self.mem) });
        }
    }

    extern "C" fn test_handler() {}

    /// Boot, run one thread at priority 1, and init a fake UARTE (on the
    /// UARTE1 line, so it cannot be confused with the real UARTE0).
    fn setup() -> (ThreadId, Fake) {
        boot();
        let t = spawn(1);
        assert_eq!(context_switch(), Some(t));
        let fake = Fake::new();
        kernel::irq_attach(Interrupt::UARTE1, test_handler);
        fake.uarte.init(Interrupt::UARTE1);
        (t, fake)
    }

    // Register map /////////////////////////////////////////////////////////

    #[test]
    fn register_offsets_match_the_nrf52840() {
        assert_eq!(offset_of!(UarteRegs, start_rx), STARTRX);
        assert_eq!(offset_of!(UarteRegs, stop_rx), 0x004);
        assert_eq!(offset_of!(UarteRegs, start_tx), STARTTX);
        assert_eq!(offset_of!(UarteRegs, stop_tx), 0x00C);
        assert_eq!(offset_of!(UarteRegs, flush_rx), 0x02C);
        assert_eq!(offset_of!(UarteRegs, cts), 0x100);
        assert_eq!(offset_of!(UarteRegs, ncts), 0x104);
        assert_eq!(offset_of!(UarteRegs, rxdrdy), 0x108);
        assert_eq!(offset_of!(UarteRegs, endrx), ENDRX);
        assert_eq!(offset_of!(UarteRegs, txdrdy), 0x11C);
        assert_eq!(offset_of!(UarteRegs, endtx), ENDTX);
        assert_eq!(offset_of!(UarteRegs, error), 0x124);
        assert_eq!(offset_of!(UarteRegs, rxto), 0x144);
        assert_eq!(offset_of!(UarteRegs, rxstarted), 0x14C);
        assert_eq!(offset_of!(UarteRegs, txstarted), 0x150);
        assert_eq!(offset_of!(UarteRegs, txstopped), 0x158);
        assert_eq!(offset_of!(UarteRegs, shorts), SHORTS);
        assert_eq!(offset_of!(UarteRegs, inten), 0x300);
        assert_eq!(offset_of!(UarteRegs, intenset), INTENSET);
        assert_eq!(offset_of!(UarteRegs, intenclr), INTENCLR);
        assert_eq!(offset_of!(UarteRegs, errorsrc), 0x480);
        assert_eq!(offset_of!(UarteRegs, enable), ENABLE);
        assert_eq!(offset_of!(UarteRegs, psel_rts), 0x508);
        assert_eq!(offset_of!(UarteRegs, psel_rxd), 0x514);
        assert_eq!(offset_of!(UarteRegs, baudrate), BAUDRATE);
        assert_eq!(offset_of!(UarteRegs, rxd) + offset_of!(Dma, ptr), RXD_PTR);
        assert_eq!(
            offset_of!(UarteRegs, rxd) + offset_of!(Dma, maxcnt),
            RXD_MAXCNT
        );
        assert_eq!(offset_of!(UarteRegs, rxd) + offset_of!(Dma, amount), 0x53C);
        assert_eq!(offset_of!(UarteRegs, txd) + offset_of!(Dma, ptr), TXD_PTR);
        assert_eq!(
            offset_of!(UarteRegs, txd) + offset_of!(Dma, maxcnt),
            TXD_MAXCNT
        );
        assert_eq!(offset_of!(UarteRegs, txd) + offset_of!(Dma, amount), 0x54C);
        assert_eq!(offset_of!(UarteRegs, config), CONFIG);
    }

    /// Known issue 14: PSEL.TXD is at 0x50C and PSEL.CTS at 0x510 (SVD and
    /// product specification); `UarteRegs` has them the other way round, so
    /// `uart_init` connects CTS to the TX pin and leaves TXD disconnected.
    #[test]
    #[ignore = "known issue 14: UarteRegs swaps PSEL.TXD and PSEL.CTS"]
    fn psel_txd_and_cts_offsets_match_the_nrf52840() {
        assert_eq!(offset_of!(UarteRegs, psel_txd), 0x50C);
        assert_eq!(offset_of!(UarteRegs, psel_cts), 0x510);
    }

    #[test]
    fn field_bits_match_the_nrf52840() {
        let int = |b: UartInterruptBuilder| b.build().into_bits();
        assert_eq!(int(UartInterruptBuilder::new().with_cts(true)), 1 << 0);
        assert_eq!(int(UartInterruptBuilder::new().with_rxdrdy(true)), 1 << 2);
        assert_eq!(int(UartInterruptBuilder::new().with_endrx(true)), INT_ENDRX);
        assert_eq!(int(UartInterruptBuilder::new().with_txdrdy(true)), 1 << 7);
        assert_eq!(int(UartInterruptBuilder::new().with_endtx(true)), INT_ENDTX);
        assert_eq!(int(UartInterruptBuilder::new().with_error(true)), 1 << 9);
        assert_eq!(int(UartInterruptBuilder::new().with_rxto(true)), 1 << 17);
        assert_eq!(
            int(UartInterruptBuilder::new().with_rx_started(true)),
            1 << 19
        );
        assert_eq!(
            int(UartInterruptBuilder::new().with_tx_started(true)),
            1 << 20
        );
        assert_eq!(
            int(UartInterruptBuilder::new().with_tx_stopped(true)),
            1 << 22
        );

        let shorts = |b: UartShortsBuilder| b.build().into_bits();
        assert_eq!(
            shorts(UartShortsBuilder::new().with_endrx_startrx(true)),
            1 << 5
        );
        assert_eq!(
            shorts(UartShortsBuilder::new().with_endrx_stoprx(true)),
            1 << 6
        );

        assert_eq!(
            UartEnableBuilder::new()
                .with_enable(true)
                .build()
                .into_bits(),
            8
        );

        let psel = UartPselBuilder::new()
            .with_pin_num(31)
            .with_port_num(1)
            .build();
        assert_eq!(psel.into_bits(), 31 | 1 << 5);
        let off = UartPselBuilder::new().with_disconnected(true).build();
        assert_eq!(off.into_bits(), 1 << 31);

        let cfg = |b: UartConfigBuilder| b.build().into_bits();
        assert_eq!(
            cfg(UartConfigBuilder::new().with_hardware_flow_control(true)),
            1
        );
        assert_eq!(
            cfg(UartConfigBuilder::new().with_parity(UartConfigParityFlag::Even)),
            0x7 << 1
        );
        assert_eq!(
            cfg(UartConfigBuilder::new().with_stop_bits(UartConfigStopBitsFlag::Two)),
            1 << 4
        );

        let err = UartErrorSrc::from_bits(0b1111);
        assert!(err.overrun() && err.parity() && err.framing() && err.r#break());
    }

    #[test]
    fn baud_rates_match_the_nrf52840() {
        let table = [
            (UartBaudRate::B1200, 0x0004_F000),
            (UartBaudRate::B9600, 0x0027_5000),
            (UartBaudRate::B14400, 0x003A_F000),
            (UartBaudRate::B115200, 0x01D6_0000),
            (UartBaudRate::B230400, 0x03B0_0000),
            (UartBaudRate::B921600, 0x0F00_0000),
            (UartBaudRate::B1M, 0x1000_0000),
        ];
        for (rate, value) in table {
            assert_eq!(rate.0, value);
        }
    }

    #[test]
    fn instances_point_at_their_peripherals() {
        assert_eq!(UARTE0.regs.as_ptr() as usize, 0x4000_2000);
        assert_eq!(UARTE1.regs.as_ptr() as usize, 0x4002_8000);
    }

    /// Known issue 3: `semaphore!` inside `Uarte::new` gives UARTE0 and
    /// UARTE1 the same two kernel objects.
    #[test]
    #[ignore = "known issue 3: semaphore! in a const fn is shared by every instance"]
    fn each_uarte_has_its_own_semaphores() {
        assert_ne!(
            UARTE0.regs_semaphore.handle(),
            UARTE1.regs_semaphore.handle()
        );
        assert_ne!(UARTE0.tx_semaphore.handle(), UARTE1.tx_semaphore.handle());
    }

    // init ///////////////////////////////////////////////////////////////////

    #[test]
    fn init_configures_pins_line_and_interrupts_and_starts_rx() {
        let _k = lock();
        let (_, f) = setup();
        let regs = &*f.uarte.regs;
        let pin = |p: UartPsel| (p.disconnected(), p.port_num(), p.pin_num());
        assert_eq!(pin(regs.psel_txd.read()), (false, 0, 0));
        assert_eq!(pin(regs.psel_rxd.read()), (false, 0, 1));
        assert!(regs.psel_rts.read().disconnected());
        assert!(regs.psel_cts.read().disconnected());
        assert_eq!(f.get(BAUDRATE), 0x01D6_0000);
        // 8N1. (Hardware flow control is on: known issue 8, DK only.)
        let config = UartConfig::from_bits(f.get(CONFIG));
        assert_eq!(config.parity(), UartConfigParityFlag::None);
        assert_eq!(config.stop_bits(), UartConfigStopBitsFlag::One);
        assert_eq!(f.get(ENABLE), 8);
        assert_eq!(nvic_priority(Interrupt::UARTE1), 0xC0);
        assert!(nvic_is_enabled(Interrupt::UARTE1));
        assert_eq!(f.get(INTENSET), INT_ENDRX);
        assert_eq!(f.get(RXD_PTR), f.uarte.rx_buffer_1.as_ptr() as u32);
        assert_eq!(f.get(RXD_MAXCNT), 1);
        assert_eq!(f.get(SHORTS), 1 << 5, "ENDRX -> STARTRX");
        assert_eq!(f.get(STARTRX), 1);
    }

    #[test]
    #[should_panic(expected = "irq_enable before irq_attach")]
    fn init_before_the_handler_is_attached_panics() {
        let _k = lock();
        let f = Fake::new();
        f.uarte.init(Interrupt::UARTE1);
    }

    // TX ////////////////////////////////////////////////////////////////////

    // On the host a blocking `take` returns at once, so `send` gives
    // `regs_semaphore` back before ENDTX arrives: these tests cannot check
    // that the lock is held across a transfer. The target suite does.

    #[test]
    fn send_programs_the_dma_and_waits_for_endtx() {
        let _k = lock();
        let (t, f) = setup();
        let msg = *b"hello";
        f.uarte.send(&msg);
        assert_eq!(f.get(TXD_PTR), msg.as_ptr() as u32);
        assert_eq!(f.get(TXD_MAXCNT), 5);
        assert_eq!(f.get(INTENSET), INT_ENDTX, "writes only the ENDTX bit");
        assert_eq!(f.get(STARTTX), 1);
        assert_eq!(thread_state(t), Some(ThreadState::Blocked), "until ENDTX");
        assert_eq!(context_switch(), Some(0));
    }

    #[test]
    fn send_clears_a_stale_endtx_before_starting() {
        let _k = lock();
        let (t, f) = setup();
        f.set(ENDTX, 1);
        f.uarte.send(b"x");
        assert_eq!(f.get(ENDTX), 0);
        assert_eq!(thread_state(t), Some(ThreadState::Blocked));
    }

    #[test]
    #[cfg_attr(miri, ignore = "known issue 9: Miri reports UB in CURR_TCB/NEXT_TCB")]
    fn endtx_interrupt_disables_itself_and_wakes_the_sender() {
        let _k = lock();
        let (t, f) = setup();
        f.uarte.send(b"hello");
        assert_eq!(context_switch(), Some(0));
        f.set(ENDTX, 1);
        f.interrupt();
        assert_eq!(f.get(ENDTX), 0, "event cleared");
        assert_eq!(f.get(INTENCLR), INT_ENDTX, "only ENDTX disabled");
        assert_eq!(thread_state(t), Some(ThreadState::Ready));
        assert!(pendsv_pending());
        assert_eq!(context_switch(), Some(t));
    }

    /// Known issue 15: MAXCNT is 16 bits; `send` saturates the count, so
    /// anything past 65535 bytes is silently not sent.
    #[test]
    fn send_of_more_than_65535_bytes_programs_only_65535() {
        let _k = lock();
        let (_, f) = setup();
        f.uarte.send(&std::vec![0u8; 70_000]);
        assert_eq!(f.get(TXD_MAXCNT), 0xFFFF);
    }

    // RX ////////////////////////////////////////////////////////////////////

    #[test]
    fn endrx_queues_the_byte_and_swaps_buffers() {
        let _k = lock();
        let (_, f) = setup();
        let (b1, b2) = (
            f.uarte.rx_buffer_1.as_ptr() as u32,
            f.uarte.rx_buffer_2.as_ptr() as u32,
        );
        f.set(STARTRX, 0);
        f.receive_byte(b'a');
        f.interrupt();
        assert_eq!(f.get(ENDRX), 0, "event cleared");
        assert_eq!(f.get(RXD_PTR), b2);
        assert_eq!(f.get(STARTRX), 1, "reception restarted");
        f.receive_byte(b'b');
        f.interrupt();
        assert_eq!(f.get(RXD_PTR), b1);
        f.receive_byte(b'c');
        f.interrupt();
        assert_eq!(f.received(), b"abc");
        assert_eq!(f.received(), b"", "receive drains the FIFO");
    }

    #[test]
    fn receive_with_nothing_received_returns_zero() {
        let _k = lock();
        let (_, f) = setup();
        let mut out = [0xEEu8; 4];
        assert_eq!(f.uarte.receive(&mut out), 0);
        assert_eq!(out, [0xEE; 4]);
    }

    #[test]
    fn receive_returns_at_most_the_buffer_size() {
        let _k = lock();
        let (_, f) = setup();
        for b in b"hello" {
            f.receive_byte(*b);
            f.interrupt();
        }
        let mut out = [0u8; 2];
        assert_eq!(f.uarte.receive(&mut out), 2);
        assert_eq!(&out, b"he");
        assert_eq!(f.received(), b"llo");
    }

    #[test]
    fn rx_overflow_keeps_the_oldest_255_bytes() {
        let _k = lock();
        let (_, f) = setup();
        for i in 0..300u32 {
            f.receive_byte(i as u8);
            f.interrupt();
        }
        let got = f.received();
        assert_eq!(got.len(), 255, "one slot of the 256 stays free");
        assert!(got.iter().enumerate().all(|(i, &b)| b == i as u8));
    }

    #[test]
    #[should_panic(expected = "Unexpected RX buffer pointer")]
    fn endrx_with_a_foreign_rx_pointer_panics() {
        let _k = lock();
        let (_, f) = setup();
        f.set(RXD_PTR, 0x2000_1234);
        f.set(ENDRX, 1);
        f.interrupt();
    }

    // Both directions ///////////////////////////////////////////////////////

    #[test]
    fn one_interrupt_handles_endtx_and_endrx_together() {
        let _k = lock();
        let (t, f) = setup();
        f.uarte.send(b"x");
        assert_eq!(context_switch(), Some(0));
        f.receive_byte(b'y');
        f.set(ENDTX, 1);
        f.interrupt();
        assert_eq!(thread_state(t), Some(ThreadState::Ready));
        assert_eq!(f.get(ENDTX), 0);
        assert_eq!(f.get(ENDRX), 0);
        assert_eq!(f.received(), b"y");
    }

    #[test]
    fn interrupt_with_no_event_changes_nothing() {
        let _k = lock();
        let (_, f) = setup();
        let regs = || {
            (0..WORDS * 4)
                .step_by(4)
                .map(|o| f.get(o))
                .collect::<Vec<u32>>()
        };
        let before = regs();
        f.set(0x104, 1); // NCTS: an event the driver does not handle
        f.interrupt();
        let mut after = regs();
        after[0x104 / 4] = 0;
        assert_eq!(before, after);
        assert!(!pendsv_pending());
        assert_eq!(f.received(), b"");
    }
}
