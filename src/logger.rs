use core::fmt::Write;

use log::*;

pub struct Logger;

impl log::Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        let mut output = arrayvec::ArrayString::<1024>::new();
        write!(
            &mut output,
            "{} {} {}\r\n",
            record.level(),
            record.target(),
            record.args()
        )
        .ok();

        cortex_m::interrupt::free(|cs| {
            let mut display_ref = crate::OLED_DISPLAY.borrow(cs).borrow_mut();
            if let Some(display) = display_ref.as_mut() {
                crate::draw_text_screen(display, output.as_str()).ok();
            }
        });
    }

    fn flush(&self) {}
}
