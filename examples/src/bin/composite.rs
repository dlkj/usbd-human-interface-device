#![no_std]
#![no_main]

use core::convert::Infallible;
use core::default::Default;

use adafruit_macropad::hal;
use cortex_m_rt::entry;
use embedded_hal::digital::v2::*;
use embedded_hal::prelude::_embedded_hal_timer_CountDown;
use embedded_time::duration::Milliseconds;
use embedded_time::rate::Hertz;
use hal::pac;
use hal::Clock;
use log::*;
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_hid_devices::device::consumer::{ConsumerControlInterface, MultipleConsumerReport};
use usbd_hid_devices::device::keyboard::NKROBootKeyboardInterface;
use usbd_hid_devices::device::mouse::{WheelMouseInterface, WheelMouseReport};
use usbd_hid_devices::page::Consumer;
use usbd_hid_devices::page::Keyboard;
use usbd_hid_devices::prelude::*;

use usbd_hid_devices_example_rp2040::*;

const KEYBOARD_MOUSE_POLL: Milliseconds = Milliseconds(10);
const CONSUMER_POLL: Milliseconds = Milliseconds(50);
const WRITE_PENDING_POLL: Milliseconds = Milliseconds(10);

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

    let timer = hal::Timer::new(pac.TIMER, &mut pac.RESETS);

    let clock = TimerClock::new(&timer);

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

    let button = pins.gpio0.into_pull_up_input();

    init_logger(oled_spi, oled_dc.into(), oled_cs.into(), &button);
    info!("Starting up...");

    //USB
    static mut USB_ALLOC: Option<UsbBusAllocator<hal::usb::UsbBus>> = None;

    //Safety: interrupts not enabled yet
    let usb_alloc = unsafe {
        USB_ALLOC = Some(UsbBusAllocator::new(hal::usb::UsbBus::new(
            pac.USBCTRL_REGS,
            pac.USBCTRL_DPRAM,
            clocks.usb_clock,
            true,
            &mut pac.RESETS,
        )));
        USB_ALLOC.as_ref().unwrap()
    };

    let mut composite = UsbHidClassBuilder::new()
        .add_interface(
            usbd_hid_devices::device::keyboard::NKROBootKeyboardInterface::default_config(&clock),
        )
        .add_interface(usbd_hid_devices::device::mouse::WheelMouseInterface::default_config())
        .add_interface(
            usbd_hid_devices::device::consumer::ConsumerControlInterface::default_config(),
        )
        //Build
        .build(usb_alloc);

    //https://pid.codes
    let mut usb_dev = UsbDeviceBuilder::new(usb_alloc, UsbVidPid(0x1209, 0x0001))
        .manufacturer("usbd-hid-devices")
        .product("Keyboard, Mouse & Consumer")
        .serial_number("TEST")
        .supports_remote_wakeup(false)
        .build();

    let mut led_pin = pins.gpio13.into_push_pull_output();
    led_pin.set_low().ok();

    let key_pins: &[&dyn InputPin<Error = core::convert::Infallible>] = &[
        &pins.gpio1.into_pull_up_input(),
        &pins.gpio2.into_pull_up_input(),
        &pins.gpio3.into_pull_up_input(),
        &pins.gpio4.into_pull_up_input(),
        &pins.gpio5.into_pull_up_input(),
        &pins.gpio6.into_pull_up_input(),
        &pins.gpio7.into_pull_up_input(),
        &pins.gpio8.into_pull_up_input(),
        &pins.gpio9.into_pull_up_input(),
        &pins.gpio10.into_pull_up_input(),
        &pins.gpio11.into_pull_up_input(),
        &pins.gpio12.into_pull_up_input(),
    ];

    let mut keyboard_mouse_poll = timer.count_down();
    keyboard_mouse_poll.start(KEYBOARD_MOUSE_POLL);

    let mut last_mouse_buttons = 0;
    let mut mouse_report = WheelMouseReport::default();

    let mut consumer_poll = timer.count_down();
    consumer_poll.start(CONSUMER_POLL);
    let mut last_consumer_report = MultipleConsumerReport::default();

    let mut write_pending_poll = timer.count_down();
    write_pending_poll.start(WRITE_PENDING_POLL);

    let mut display_poll = timer.count_down();
    display_poll.start(DISPLAY_POLL);

    let mut tick_count_down = timer.count_down();
    tick_count_down.start(Milliseconds(1));

    loop {
        if button.is_low().unwrap() {
            hal::rom_data::reset_to_usb_boot(0x1 << 13, 0x0);
        }

        if keyboard_mouse_poll.wait().is_ok() {
            let keys = get_keyboard_keys(key_pins);

            let keyboard = composite.interface::<NKROBootKeyboardInterface<'_, _, _>, _>();
            match keyboard.write_report(keys) {
                Err(UsbHidError::WouldBlock) => {}
                Err(UsbHidError::Duplicate) => {}
                Ok(_) => {}
                Err(e) => {
                    panic!("Failed to write keyboard report: {:?}", e)
                }
            };

            mouse_report = update_mouse_report(mouse_report, key_pins);
            if mouse_report.buttons != last_mouse_buttons
                || mouse_report.x != 0
                || mouse_report.y != 0
            {
                let mouse = composite.interface::<WheelMouseInterface<'_, _>, _>();
                match mouse.write_report(&mouse_report) {
                    Err(UsbHidError::WouldBlock) => {}
                    Ok(_) => {
                        last_mouse_buttons = mouse_report.buttons;
                        mouse_report = Default::default();
                    }
                    Err(e) => {
                        panic!("Failed to write mouse report: {:?}", e)
                    }
                };
            }
        }

        if consumer_poll.wait().is_ok() {
            let codes = get_consumer_codes(key_pins);
            let consumer_report = MultipleConsumerReport {
                codes: [
                    codes[0],
                    codes[1],
                    Consumer::Unassigned,
                    Consumer::Unassigned,
                ],
            };

            if last_consumer_report != consumer_report {
                let consumer = composite.interface::<ConsumerControlInterface<'_, _>, _>();
                match consumer.write_report(&consumer_report) {
                    Err(UsbError::WouldBlock) => {}
                    Ok(_) => {
                        last_consumer_report = consumer_report;
                    }
                    Err(e) => {
                        panic!("Failed to write consumer report: {:?}", e)
                    }
                };
            }
        }

        //Tick once per ms
        if tick_count_down.wait().is_ok() {
            match composite
                .interface::<NKROBootKeyboardInterface<'_, _, _>, _>()
                .tick()
            {
                Err(UsbHidError::WouldBlock) => {}
                Ok(_) => {}
                Err(e) => {
                    panic!("Failed to process keyboard tick: {:?}", e)
                }
            };
        }

        if usb_dev.poll(&mut [&mut composite]) {
            let keyboard = composite.interface::<NKROBootKeyboardInterface<'_, _, _>, _>();
            match keyboard.read_report() {
                Err(UsbError::WouldBlock) => {}
                Err(e) => {
                    panic!("Failed to read keyboard report: {:?}", e)
                }
                Ok(leds) => {
                    led_pin.set_state(PinState::from(leds.num_lock)).ok();
                }
            }
        }

        if display_poll.wait().is_ok() {
            log::logger().flush();
        }
    }
}

fn get_keyboard_keys(keys: &[&dyn InputPin<Error = Infallible>]) -> [Keyboard; 3] {
    [
        if keys[9].is_low().unwrap() {
            Keyboard::A
        } else {
            Keyboard::NoEventIndicated
        },
        if keys[10].is_low().unwrap() {
            Keyboard::B
        } else {
            Keyboard::NoEventIndicated
        },
        if keys[11].is_low().unwrap() {
            Keyboard::C
        } else {
            Keyboard::NoEventIndicated
        },
    ]
}

fn get_consumer_codes(keys: &[&dyn InputPin<Error = Infallible>]) -> [Consumer; 2] {
    [
        if keys[3].is_low().unwrap() {
            Consumer::VolumeDecrement
        } else {
            Consumer::Unassigned
        },
        if keys[5].is_low().unwrap() {
            Consumer::VolumeIncrement
        } else {
            Consumer::Unassigned
        },
    ]
}

fn update_mouse_report(
    mut report: WheelMouseReport,
    keys: &[&dyn InputPin<Error = core::convert::Infallible>],
) -> WheelMouseReport {
    if keys[0].is_low().unwrap() {
        report.buttons |= 0x1; //Left
    } else {
        report.buttons &= 0xFF - 0x1;
    }
    if keys[1].is_low().unwrap() {
        report.buttons |= 0x4; //Middle
    } else {
        report.buttons &= 0xFF - 0x4;
    }
    if keys[2].is_low().unwrap() {
        report.buttons |= 0x2; //Right
    } else {
        report.buttons &= 0xFF - 0x2;
    }
    if keys[4].is_low().unwrap() {
        report.y = i8::saturating_add(report.y, -10); //Up
    }
    if keys[6].is_low().unwrap() {
        report.x = i8::saturating_add(report.x, -10); //Left
    }
    if keys[7].is_low().unwrap() {
        report.y = i8::saturating_add(report.y, 10); //Down
    }
    if keys[8].is_low().unwrap() {
        report.x = i8::saturating_add(report.x, 10); //Right
    }

    report
}
