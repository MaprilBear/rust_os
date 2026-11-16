use crate::{os_sleep, uarte::UARTE0};

pub extern "C" fn user_thread1() -> ! {
    loop {
        //UARTE0.send(b"Hello, UART!\n");
        os_sleep(1);
    }
}

pub extern "C" fn user_thread2() -> ! {
    loop {
        let mut buffer: [u8; 8] = [0; 8];
        let chars = UARTE0.receive(&mut buffer);
        if chars != 0 {
            let input = core::str::from_utf8(&buffer).unwrap_or("Bad input");
            UARTE0.send(input.as_bytes());
        }
    }
}
