use core::cell::{Cell, RefCell};
use core::fmt::Write;

use arrayvec::ArrayString;
use cortex_m::interrupt::Mutex;
use log::*;

pub struct Logger {
    pub buffer: Mutex<RefCell<arrayvec::ArrayString<1024>>>,
    pub updated: Mutex<Cell<bool>>,
}

impl log::Log for Logger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        cortex_m::interrupt::free(|cs| {
            let buffer_ref = self.buffer.borrow(cs);
            while buffer_ref.borrow().len() > ((buffer_ref.borrow().capacity() / 4) * 3) {
                buffer_ref.replace_with(|b| Self::truncate_buffer(b));
            }

            writeln!(
                &mut buffer_ref.borrow_mut(),
                "{} {} {}",
                Self::level_str(record.level()),
                record
                    .target()
                    .split("::")
                    .last()
                    .unwrap_or_else(|| record.target()),
                record.args()
            )
            .ok();
            self.updated.borrow(cs).set(true);
        });
    }

    fn flush(&self) {
        cortex_m::interrupt::free(|cs| {
            if !self.updated.borrow(cs).replace(false) {
                return;
            }

            //Truncate buffer on draw to minimise likelihood of truncate on log
            let buffer_ref = self.buffer.borrow(cs);

            buffer_ref.replace_with(|b| Self::truncate_buffer(b));

            let mut display_ref = crate::OLED_DISPLAY.borrow(cs).borrow_mut();
            if let Some(display) = display_ref.as_mut() {
                crate::draw_text_screen(display, buffer_ref.borrow().as_str()).ok();
            }
        });
    }
}

impl Logger {
    fn level_str(level: Level) -> &'static str {
        match level {
            Level::Error => "E",
            Level::Warn => "W",
            Level::Info => "I",
            Level::Debug => "D",
            Level::Trace => "T",
        }
    }

    fn truncate_buffer(buffer: &ArrayString<1024>) -> ArrayString<1024> {
        if buffer.len() < buffer.capacity() / 2 {
            return *buffer;
        }

        let s = &buffer.as_str()[(buffer.len() - (buffer.capacity() / 2))..];
        ArrayString::from(s).unwrap()
    }
}
