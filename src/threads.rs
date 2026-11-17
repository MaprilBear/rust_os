use crate::{
    syscall::{sys_sleep, sys_yield},
    uarte::UARTE0,
};

#[link_section = ".user_text"]
pub extern "C" fn user_thread1() -> ! {
    loop {
        //sys_uart_write(b"Hello, UART!\n");
        sys_sleep(1);
    }
}

#[link_section = ".user_text"]
pub extern "C" fn user_thread2() -> ! {
    loop {
        let mut buffer: [u8; 8] = [0; 8];
        let chars = UARTE0.receive(&mut buffer) as usize;
        if chars != 0 {
            let input = core::str::from_utf8(&buffer[..chars]).unwrap_or("Bad input");
            UARTE0.send(input.as_bytes());
        } else {
            sys_yield();
        }
    }
}
