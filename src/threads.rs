use crate::uarte::UARTE0;

pub extern "C" fn idle_thread() -> ! {
    loop {
        UARTE0.send(b"Hello, UART!\n");
    }
}

pub extern "C" fn idle_thread2() -> ! {
    loop {
        UARTE0.send(b"Screw you, UART!\n");
    }
}
