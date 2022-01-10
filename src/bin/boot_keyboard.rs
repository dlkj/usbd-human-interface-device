#![no_std]
#![no_main]

use adafruit_macropad::hal;
use cortex_m_rt::entry;
use embedded_hal::digital::v2::*;
use embedded_time::rate::Hertz;
use hal::pac;
use hal::Clock;
use log::*;
use sh1106::prelude::*;
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_hid_devices::keyboard::HIDKeyboard;
use usbd_hid_devices_example_rp2040::logger::MacropadLogger;

#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_GD25Q64CS;

static LOGGER: MacropadLogger = MacropadLogger;

const XTAL_FREQ_HZ: u32 = 12_000_000u32;

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
        usbd_hid_devices_example_rp2040::logger::OLED_DISPLAY
            .borrow(cs)
            .replace(Some(display));
        unsafe {
            log::set_logger_racy(&LOGGER)
                .map(|()| log::set_max_level(LevelFilter::Info))
                .unwrap();
        }
    });

    info!("Starting up...");

    //USB
    let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
        pac.USBCTRL_REGS,
        pac.USBCTRL_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));

    let mut keyboard = usbd_hid_devices::hid::HID::new(
        &usb_bus,
        usbd_hid_devices::keyboard::HIDBootKeyboard::default(),
    );

    //https://pid.codes
    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x1209, 0x0001))
        .manufacturer("DLKJ")
        .product("Keyboard")
        .serial_number("TEST")
        .device_class(3) // HID - from: https://www.usb.org/defined-class-codes
        .composite_with_iads()
        .supports_remote_wakeup(true)
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
                if in11.is_low().unwrap() { 0x28 } else { 0x00 }, //Enter
            ];

            keyboard.write_keycodes(keys).ok();
        }
    }
}
