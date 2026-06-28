use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1, KeyCode};

pub struct YieldNow {
    yielded: bool,
}

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.yielded {
            Poll::Ready(())
        } else {
            self.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

pub fn yield_now() -> YieldNow {
    YieldNow { yielded: false }
}

pub async fn keyboard_task() {
    let mut keyboard = Keyboard::new(ScancodeSet1::new(), layouts::Us104Key, HandleControl::Ignore);

    loop {

        let mut port_64 = x86_64::instructions::port::Port::<u8>::new(0x64);
        let mut port_60 = x86_64::instructions::port::Port::<u8>::new(0x60);

        let mut read_something = false;
        
        unsafe {
            // Drain the buffer
            while (port_64.read() & 1) == 1 {
                read_something = true;
                let scancode = port_60.read();

                if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
                    if let Some(key) = keyboard.process_keyevent(key_event) {
                        match key {
                            DecodedKey::RawKey(raw) => {
                                match raw {
                                    KeyCode::PageUp | KeyCode::ArrowUp => crate::writer::scroll_up(),
                                    KeyCode::PageDown | KeyCode::ArrowDown => crate::writer::scroll_down(),
                                    _ => {}
                                }
                            }
                            DecodedKey::Unicode(character) => {
                                crate::print!("{}", character);
                            }
                        }
                    }
                }
            }
        }
        
        // If we didn't read anything, yield to other tasks to prevent monopolizing CPU
        if !read_something {
            unsafe { core::arch::asm!("pause") };
            yield_now().await;
        }
    }
}
