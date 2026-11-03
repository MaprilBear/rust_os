use crate::{os_sleep, uarte::UARTE0};

pub extern "C" fn user_thread1() -> ! {
    loop {
        UARTE0.send(b"Hello, UART!\n");
        os_sleep(1);
    }
}

pub extern "C" fn user_thread2() -> ! {
    loop {
        UARTE0.send(b"Screw you, UART!\n");
        os_sleep(4);
    }
}
