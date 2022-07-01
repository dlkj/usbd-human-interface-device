//! # HID USB Examples for usbd-human-interface-device

#![no_std]

use core::cell::{Cell, RefCell};

use crate::hal::spi::Enabled;
use crate::hal::Timer;
use crate::pac::SPI1;
use adafruit_macropad::hal;
use arrayvec::ArrayString;
use cortex_m::interrupt::Mutex;
use embedded_graphics::mono_font::ascii::FONT_4X6;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use embedded_hal::digital::v2::InputPin;
use embedded_text::alignment::VerticalAlignment;
use embedded_text::style::VerticalOverdraw;
use embedded_text::{
    alignment::HorizontalAlignment,
    style::{HeightMode, TextBoxStyleBuilder},
    TextBox,
};
use embedded_time::clock::Error;
use embedded_time::duration::{Fraction, Milliseconds};
use embedded_time::Instant;
use hal::gpio::DynPin;
use hal::pac;
use hal::Spi;
use log::LevelFilter;
use sh1106::prelude::*;

pub mod logger;

pub const XTAL_FREQ_HZ: u32 = 12_000_000u32;
pub const MAX_LOG_LEVEL: LevelFilter = LevelFilter::Debug;
pub const DISPLAY_POLL: Milliseconds = Milliseconds(200);
pub const SCALING_FACTOR: Fraction = Fraction::new(1, 1_000_000u32);

pub struct TimerClock<'a> {
    timer: &'a Timer,
}

impl<'a> TimerClock<'a> {
    pub fn new(timer: &'a Timer) -> Self {
        Self { timer }
    }
}

impl<'a> embedded_time::clock::Clock for TimerClock<'a> {
    type T = u32;
    const SCALING_FACTOR: Fraction = SCALING_FACTOR;

    fn try_now(&self) -> Result<Instant<Self>, Error> {
        Ok(Instant::new(self.timer.get_counter_low()))
    }
}

pub struct SyncTimerClock {
    timer: Mutex<Timer>,
}

impl SyncTimerClock {
    pub fn new(timer: Timer) -> Self {
        Self {
            timer: Mutex::new(timer),
        }
    }
}

impl embedded_time::clock::Clock for SyncTimerClock {
    type T = u32;

    const SCALING_FACTOR: Fraction = SCALING_FACTOR;

    fn try_now(&self) -> Result<Instant<Self>, Error> {
        cortex_m::interrupt::free(|cs| Ok(Instant::new(self.timer.borrow(cs).get_counter_low())))
    }
}

pub static LOGGER: logger::Logger = logger::Logger {
    buffer: Mutex::new(RefCell::new(ArrayString::new_const())),
    updated: Mutex::new(Cell::new(false)),
};

type DisplaySpiInt = SpiInterface<Spi<hal::spi::Enabled, pac::SPI1, 8_u8>, DynPin, DynPin>;

static OLED_DISPLAY: Mutex<RefCell<Option<GraphicsMode<DisplaySpiInt>>>> =
    Mutex::new(RefCell::new(None));

pub fn init_logger<E>(
    oled_spi: Spi<Enabled, SPI1, 8>,
    oled_dc: DynPin,
    oled_cs: DynPin,
    button: &dyn InputPin<Error = E>,
) {
    init_display(oled_spi, oled_dc, oled_cs);
    check_for_persisted_panic(button);
}

fn check_for_persisted_panic<E>(restart: &dyn InputPin<Error = E>) {
    if let Some(msg) = panic_persist::get_panic_message_utf8() {
        cortex_m::interrupt::free(|cs| {
            let mut display_ref = OLED_DISPLAY.borrow(cs).borrow_mut();
            if let Some(display) = display_ref.as_mut() {
                draw_text_screen(display, msg).ok();
            }
        });

        while restart.is_high().unwrap_or(false) {
            cortex_m::asm::nop()
        }

        //USB boot with pin 13 for usb activity
        //Screen will continue to show panic message
        hal::rom_data::reset_to_usb_boot(0x1 << 13, 0x0);
    }
}

fn draw_text_screen<DI, E>(display: &mut GraphicsMode<DI>, text: &str) -> Result<(), E>
where
    DI: sh1106::interface::DisplayInterface<Error = E>,
{
    display.clear();
    let character_style = MonoTextStyle::new(&FONT_4X6, BinaryColor::On);
    let text_box_style = TextBoxStyleBuilder::new()
        .height_mode(HeightMode::Exact(VerticalOverdraw::FullRowsOnly))
        .alignment(HorizontalAlignment::Left)
        .vertical_alignment(VerticalAlignment::Bottom)
        .build();
    let bounds = Rectangle::new(Point::zero(), Size::new(128, 64));
    let text_box = TextBox::with_textbox_style(text, bounds, character_style, text_box_style);

    text_box.draw(display).unwrap();
    display.flush()?;

    Ok(())
}

fn init_display(spi: Spi<hal::spi::Enabled, pac::SPI1, 8_u8>, dc: DynPin, cs: DynPin) {
    let mut display: GraphicsMode<_> = sh1106::Builder::new().connect_spi(spi, dc, cs).into();

    display.init().unwrap();
    display.flush().unwrap();

    cortex_m::interrupt::free(|cs| {
        OLED_DISPLAY.borrow(cs).replace(Some(display));
        unsafe {
            log::set_logger_racy(&LOGGER)
                .map(|()| log::set_max_level(MAX_LOG_LEVEL))
                .unwrap();
        }
    });
}
