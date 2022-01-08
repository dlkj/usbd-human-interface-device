//! # HID USB Example for usbd-hid-devices

#![no_std]
#![no_main]

use adafruit_macropad::hal;
use core::cell::RefCell;
use core::fmt::Write;
use core::panic::PanicInfo;
use core::sync::atomic::{self, Ordering};
use cortex_m::interrupt::Mutex;
use cortex_m_rt::entry;
use embedded_graphics::mono_font::ascii::FONT_4X6;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use embedded_hal::digital::v2::InputPin;
use embedded_hal::digital::v2::OutputPin;
use embedded_hal::digital::v2::PinState;
use embedded_hal::digital::v2::ToggleableOutputPin;
use embedded_hal::prelude::*;
use embedded_text::{
    alignment::HorizontalAlignment,
    style::{HeightMode, TextBoxStyleBuilder},
    TextBox,
};
use embedded_time::duration::*;
use embedded_time::rate::Hertz;
use hal::gpio::DynPin;
use hal::pac;
use hal::Clock;
use hal::Spi;
use hal::Timer;
use log::*;

use sh1106::prelude::*;
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_hid_devices::keyboard::HIDKeyboard;

#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_GD25Q64CS;

const XTAL_FREQ_HZ: u32 = 12_000_000u32;

static OLED_DISPLAY: Mutex<
    RefCell<
        Option<GraphicsMode<SpiInterface<Spi<hal::spi::Enabled, pac::SPI1, 8_u8>, DynPin, DynPin>>>,
    >,
> = Mutex::new(RefCell::new(None));

static LOGGER: MacropadLogger = MacropadLogger;

#[entry]
fn main() -> ! {
    let mut pac = pac::Peripherals::take().unwrap();

    let mut watchdog = hal::Watchdog::new(pac.WATCHDOG);
    let clocks = hal::clocks::init_clocks_and_plls(
        XTAL_FREQ_HZ,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    let sio = hal::Sio::new(pac.SIO);
    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    //display
    // These are implicitly used by the spi driver if they are in the correct mode
    let _spi_sclk = pins.gpio26.into_mode::<hal::gpio::FunctionSpi>();
    let _spi_mosi = pins.gpio27.into_mode::<hal::gpio::FunctionSpi>();
    let _spi_miso = pins.gpio28.into_mode::<hal::gpio::FunctionSpi>();
    let spi = hal::spi::Spi::<_, _, 8>::new(pac.SPI1);

    // Display control pins
    let oled_dc = pins.gpio24.into_push_pull_output();
    let oled_cs = pins.gpio22.into_push_pull_output();
    let mut oled_reset = pins.gpio23.into_push_pull_output();

    oled_reset.set_high().ok(); //disable screen reset

    // Exchange the uninitialised SPI driver for an initialised one
    let oled_spi = spi.init(
        &mut pac.RESETS,
        clocks.peripheral_clock.freq(),
        Hertz::new(16_000_000u32),
        &embedded_hal::spi::MODE_0,
    );

    let mut display: GraphicsMode<_> = sh1106::Builder::new()
        .connect_spi(oled_spi, oled_dc.into(), oled_cs.into())
        .into();

    display.init().unwrap();
    display.flush().unwrap();

    cortex_m::interrupt::free(|cs| {
        OLED_DISPLAY.borrow(cs).replace(Some(display));

        let mut display_ref = OLED_DISPLAY.borrow(cs).borrow_mut();
        let display = display_ref.as_mut().unwrap();
        draw_text_screen(display, "Starting up...").ok();

        unsafe {
            log::set_logger_racy(&LOGGER)
                .map(|()| log::set_max_level(LevelFilter::Info))
                .unwrap();
        }
    });

    //USB
    let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
        pac.USBCTRL_REGS,
        pac.USBCTRL_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));

    let mut keyboard =
        usbd_hid_devices::keyboard::HIDBootKeyboard::new(&usb_bus, 10.milliseconds());

    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x16c0, 0x27ee))
        .manufacturer("DLKJ")
        .product("Keyboard")
        .serial_number("TEST")
        .device_class(3) // HID - from: https://www.usb.org/defined-class-codes
        .build();

    //GPIO pins
    let mut led_pin = pins.gpio13.into_push_pull_output();

    let in0 = pins.gpio1.into_pull_up_input();
    let in1 = pins.gpio2.into_pull_up_input();
    let in2 = pins.gpio3.into_pull_up_input();
    let in3 = pins.gpio4.into_pull_up_input();
    let in4 = pins.gpio5.into_pull_up_input();
    let in5 = pins.gpio6.into_pull_up_input();
    let in6 = pins.gpio7.into_pull_up_input();
    let in7 = pins.gpio8.into_pull_up_input();
    let in8 = pins.gpio9.into_pull_up_input();
    let in9 = pins.gpio10.into_pull_up_input();
    let in10 = pins.gpio11.into_pull_up_input();
    let in11 = pins.gpio12.into_pull_up_input();

    let timer = Timer::new(pac.TIMER, &mut pac.RESETS);
    let mut count_down = timer.count_down();
    count_down.start(500.milliseconds());

    for _ in [0..10] {
        led_pin.toggle().unwrap();
        while let Err(_) = count_down.wait() {
            cortex_m::asm::nop();
        }
        count_down.start(500.milliseconds())
    }

    led_pin.set_low().ok();

    loop {
        if usb_dev.poll(&mut [&mut keyboard]) {
            match keyboard.read_leds() {
                Err(_) => {
                    //do nothing
                }
                Ok(leds) => {
                    //send scroll lock to the led
                    led_pin.set_state(PinState::from((leds & 0x1) != 0)).ok();
                }
            }

            let keys = [
                if in0.is_low().unwrap() { 0x53 } else { 0x00 }, //Numlock
                if in1.is_low().unwrap() { 0x52 } else { 0x00 }, //Up
                if in2.is_low().unwrap() { 0x45 } else { 0x00 }, //F12
                if in3.is_low().unwrap() { 0x50 } else { 0x00 }, //Left
                if in4.is_low().unwrap() { 0x51 } else { 0x00 }, //Down
                if in5.is_low().unwrap() { 0x4F } else { 0x00 }, //Right
                if in6.is_low().unwrap() { 0x04 } else { 0x00 }, //A
                if in7.is_low().unwrap() { 0x05 } else { 0x00 }, //B
                if in8.is_low().unwrap() { 0x06 } else { 0x00 }, //C
                if in9.is_low().unwrap() { 0xE0 } else { 0x00 }, //LCtrl
                if in10.is_low().unwrap() { 0xE1 } else { 0x00 }, //LShift
                if in11.is_low().unwrap() { 0xE2 } else { 0x00 }, //LAlt
            ];

            keyboard.write_keycodes(keys).ok();
        }
    }
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

#[inline(never)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let mut output = arrayvec::ArrayString::<1024>::new();
    if write!(&mut output, "{}", info).ok().is_some() {
        cortex_m::interrupt::free(|cs| {
            let mut display_ref = OLED_DISPLAY.borrow(cs).borrow_mut();
            if let Some(display) = display_ref.as_mut() {
                draw_text_screen(display, output.as_str()).ok();
            }
        });
    }

    loop {
        atomic::compiler_fence(Ordering::SeqCst);
    }
}
