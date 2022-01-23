use arrayvec::ArrayString;
use core::cell::RefCell;
use core::fmt::Write;
use cortex_m::interrupt::Mutex;

use log::*;

pub struct Logger {
    pub buffer: Mutex<RefCell<arrayvec::ArrayString<1024>>>,
}

impl log::Log for Logger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        cortex_m::interrupt::free(|cs| {
            //truncate buffer if more than half full
            let buffer_ref = self.buffer.borrow(cs);
            while buffer_ref.borrow().len() > (buffer_ref.borrow().capacity() / 2) {
                let buffer = buffer_ref.borrow();
                let s = &buffer.as_str()[..(buffer.capacity() / 2)];
                buffer_ref.replace(ArrayString::from(s).unwrap());
            }

            write!(
                &mut buffer_ref.borrow_mut(),
                "{} {} {}\r\n",
                record.level(),
                record.target(),
                record.args()
            )
            .ok();

            let mut display_ref = crate::OLED_DISPLAY.borrow(cs).borrow_mut();
            if let Some(display) = display_ref.as_mut() {
                crate::draw_text_screen(display, buffer_ref.borrow().as_str()).ok();
            }
        });
    }

    fn flush(&self) {}
}
