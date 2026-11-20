//! The UARTE driver on the (simulated) peripheral: transmit, receive, and the
//! TX lock across threads. Its Robot test (renode/tests/drivers.robot)
//! watches UART0's output and types its input.
#![no_std]
#![no_main]

use drivers::uarte::{self, UARTE0};
use ktest::tests;
use userlib::{sys_sleep, Semaphore};

static GO_SENDER: [Semaphore; 2] = [kernel::semaphore!(0), kernel::semaphore!(0)];
static DONE_SENDER: Semaphore = kernel::semaphore!(0);

/// Lines the senders transmit, 5 times each. Robot checks each arrives whole.
const LINES: [&[u8]; 2] = [
    b"<AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA>\n",
    b"<BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB>\n",
];

#[no_mangle]
pub extern "C" fn user_main() {
    kernel::irq_attach(kernel::Interrupt::UARTE0_UART0, uarte::uarte0_handler);
    uarte::uart_init();
    kernel::os_add_thread(sender0, 512, ktest::WORKER_PRIORITY);
    kernel::os_add_thread(sender1, 512, ktest::WORKER_PRIORITY);
    ktest::start(
        "driver_uarte",
        tests![
            send_returns_once_the_transfer_is_done,
            concurrent_sends_do_not_interleave,
            receive_gets_the_typed_bytes = 10000,
            receive_keeps_the_rest_queued = 10000,
        ],
    );
}

fn sender(i: usize) -> ! {
    loop {
        GO_SENDER[i].take();
        for _ in 0..5 {
            // From RAM: EasyDMA cannot read flash (known issue 6).
            let mut line = [0u8; 35];
            line.copy_from_slice(LINES[i]);
            UARTE0.send(&line);
        }
        DONE_SENDER.give();
    }
}
extern "C" fn sender0() -> ! {
    sender(0)
}
extern "C" fn sender1() -> ! {
    sender(1)
}

fn send_returns_once_the_transfer_is_done() {
    let line = *b"uarte: hello\n";
    UARTE0.send(&line);
    UARTE0.send(&line);
}

/// `send` holds the driver's lock for the whole DMA transfer, so each of the
/// two threads' lines comes out, whole. (Robot checks the UART output.
/// Renode sends a DMA buffer at once, so without the lock lines are lost
/// rather than mixed: an ENDTX wake-up goes to the wrong sender.)
fn concurrent_sends_do_not_interleave() {
    for go in &GO_SENDER {
        go.give();
    }
    for _ in &GO_SENDER {
        DONE_SENDER.take();
    }
}

/// Collect `n` bytes from the RX FIFO, polling.
fn receive_exactly(buf: &mut [u8]) {
    let mut got = 0;
    while got < buf.len() {
        got += UARTE0.receive(&mut buf[got..]) as usize;
        if got < buf.len() {
            sys_sleep(1);
        }
    }
}

/// Robot types "ktest" once this test starts.
fn receive_gets_the_typed_bytes() {
    let mut buf = [0u8; 5];
    receive_exactly(&mut buf);
    assert_eq!(&buf, b"ktest");
}

/// Robot types "0123456789"; a short read leaves the rest queued.
fn receive_keeps_the_rest_queued() {
    let mut buf = [0u8; 10];
    receive_exactly(&mut buf[..4]);
    assert_eq!(&buf[..4], b"0123");
    let mut rest = [0u8; 6];
    receive_exactly(&mut rest);
    assert_eq!(&rest, b"456789");
}
