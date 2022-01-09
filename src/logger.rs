use adafruit_macropad::hal;
use core::cell::RefCell;
use core::fmt::Write;
use cortex_m::interrupt::Mutex;
use embedded_graphics::mono_font::ascii::FONT_4X6;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use embedded_text::{
    alignment::HorizontalAlignment,
    style::{HeightMode, TextBoxStyleBuilder},
    TextBox,
};
use hal::gpio::DynPin;
use hal::pac;
use hal::Spi;
use log::*;
use sh1106::prelude::*;

type DisplaySpiInt = SpiInterface<Spi<hal::spi::Enabled, pac::SPI1, 8_u8>, DynPin, DynPin>;

pub static OLED_DISPLAY: Mutex<RefCell<Option<GraphicsMode<DisplaySpiInt>>>> =
    Mutex::new(RefCell::new(None));

pub struct MacropadLogger;

impl log::Log for MacropadLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
        //metadata.level() <= Level::Info
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
            let mut display_ref = OLED_DISPLAY.borrow(cs).borrow_mut();
            if let Some(display) = display_ref.as_mut() {
                draw_text_screen(display, output.as_str()).ok();
            }
        });
    }

    fn flush(&self) {}
}

pub fn draw_text_screen<DI, E>(display: &mut GraphicsMode<DI>, text: &str) -> Result<(), E>
where
    DI: sh1106::interface::DisplayInterface<Error = E>,
{
    display.clear();
    let character_style = MonoTextStyle::new(&FONT_4X6, BinaryColor::On);
    let textbox_style = TextBoxStyleBuilder::new()
        .height_mode(HeightMode::FitToText)
        .alignment(HorizontalAlignment::Left)
        .build();
    let bounds = Rectangle::new(Point::zero(), Size::new(128, 0));
    let text_box = TextBox::with_textbox_style(text, bounds, character_style, textbox_style);

    text_box.draw(display).unwrap();
    display.flush()?;

    Ok(())
}
