#![no_std]
#![no_main]

use core::convert::Infallible;
use core::default::Default;

use bsp::entry;
use bsp::hal;
use defmt::*;
use defmt_rtt as _;
use embedded_hal::digital::v2::*;
use embedded_hal::prelude::_embedded_hal_timer_CountDown;
use fugit::ExtU32;
use fugit::MicrosDurationU32;
use hal::pac;
use panic_probe as _;
#[allow(clippy::wildcard_imports)]
use usb_device::class_prelude::*;
use usb_device::prelude::*;

use usbd_human_interface_device::device::consumer::{ConsumerControl, MultipleConsumerReport};
use usbd_human_interface_device::device::keyboard::NKROBootKeyboard;
use usbd_human_interface_device::device::mouse::{WheelMouse, WheelMouseReport};
use usbd_human_interface_device::page::Consumer;
use usbd_human_interface_device::page::Keyboard;
use usbd_human_interface_device::prelude::*;

use rp_pico as bsp;

const KEYBOARD_MOUSE_POLL: MicrosDurationU32 = MicrosDurationU32::millis(10);
const CONSUMER_POLL: MicrosDurationU32 = MicrosDurationU32::millis(50);
const WRITE_PENDING_POLL: MicrosDurationU32 = MicrosDurationU32::millis(10);

#[entry]
fn main() -> ! {
    let mut pac = pac::Peripherals::take().unwrap();

    let mut watchdog = hal::Watchdog::new(pac.WATCHDOG);
    let clocks = hal::clocks::init_clocks_and_plls(
        bsp::XOSC_CRYSTAL_FREQ,
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

    let sio = hal::Sio::new(pac.SIO);
    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    info!("Starting");

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

    let mut multi_device = UsbHidClassBuilder::new()
        .add_device(
            usbd_human_interface_device::device::keyboard::NKROBootKeyboardConfig::default(),
        )
        .add_device(usbd_human_interface_device::device::mouse::WheelMouseConfig::default())
        .add_device(usbd_human_interface_device::device::consumer::ConsumerControlConfig::default())
        //Build
        .build(usb_alloc);

    //https://pid.codes
    let mut usb_dev = UsbDeviceBuilder::new(usb_alloc, UsbVidPid(0x1209, 0x0001))
        .manufacturer("usbd-human-interface-device")
        .product("Keyboard, Mouse & Consumer")
        .serial_number("TEST")
        .build();

    let mut led_pin = pins.gpio13.into_push_pull_output();
    led_pin.set_low().ok();

    let keyboard_pins: [&dyn InputPin<Error = core::convert::Infallible>; 3] = [
        &pins.gpio10.into_pull_up_input(),
        &pins.gpio11.into_pull_up_input(),
        &pins.gpio12.into_pull_up_input(),
    ];

    let mouse_pins: [&dyn InputPin<Error = core::convert::Infallible>; 7] = [
        &pins.gpio1.into_pull_up_input(),
        &pins.gpio2.into_pull_up_input(),
        &pins.gpio3.into_pull_up_input(),
        &pins.gpio5.into_pull_up_input(),
        &pins.gpio7.into_pull_up_input(),
        &pins.gpio8.into_pull_up_input(),
        &pins.gpio9.into_pull_up_input(),
    ];

    let consumer_pins: [&dyn InputPin<Error = core::convert::Infallible>; 2] = [
        &pins.gpio4.into_pull_up_input(),
        &pins.gpio6.into_pull_up_input(),
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

    let mut tick_count_down = timer.count_down();
    tick_count_down.start(1.millis());

    loop {
        if keyboard_mouse_poll.wait().is_ok() {
            let keys = get_keyboard_keys(&keyboard_pins);

            let keyboard = multi_device.device::<NKROBootKeyboard<'_, _>, _>();
            match keyboard.write_report(keys) {
                Err(UsbHidError::WouldBlock) => {}
                Err(UsbHidError::Duplicate) => {}
                Ok(_) => {}
                Err(e) => {
                    core::panic!("Failed to write keyboard report: {:?}", e)
                }
            };

            mouse_report = update_mouse_report(mouse_report, &mouse_pins);
            if mouse_report.buttons != last_mouse_buttons
                || mouse_report.x != 0
                || mouse_report.y != 0
            {
                let mouse = multi_device.device::<WheelMouse<'_, _>, _>();
                match mouse.write_report(&mouse_report) {
                    Err(UsbHidError::WouldBlock) => {}
                    Ok(_) => {
                        last_mouse_buttons = mouse_report.buttons;
                        mouse_report = Default::default();
                    }
                    Err(e) => {
                        core::panic!("Failed to write mouse report: {:?}", e)
                    }
                };
            }
        }

        if consumer_poll.wait().is_ok() {
            let codes = get_consumer_codes(&consumer_pins);
            let consumer_report = MultipleConsumerReport {
                codes: [
                    codes[0],
                    codes[1],
                    Consumer::Unassigned,
                    Consumer::Unassigned,
                ],
            };

            if last_consumer_report != consumer_report {
                let consumer = multi_device.device::<ConsumerControl<'_, _>, _>();
                match consumer.write_report(&consumer_report) {
                    Err(UsbError::WouldBlock) => {}
                    Ok(_) => {
                        last_consumer_report = consumer_report;
                    }
                    Err(e) => {
                        core::panic!("Failed to write consumer report: {:?}", e)
                    }
                };
            }
        }

        //Tick once per ms
        if tick_count_down.wait().is_ok() {
            match multi_device.tick() {
                Err(UsbHidError::WouldBlock) => {}
                Ok(_) => {}
                Err(e) => {
                    core::panic!("Failed to process keyboard tick: {:?}", e)
                }
            };
        }

        if usb_dev.poll(&mut [&mut multi_device]) {
            let keyboard = multi_device.device::<NKROBootKeyboard<'_, _>, _>();
            match keyboard.read_report() {
                Err(UsbError::WouldBlock) => {}
                Err(e) => {
                    core::panic!("Failed to read keyboard report: {:?}", e)
                }
                Ok(leds) => {
                    led_pin.set_state(PinState::from(leds.num_lock)).ok();
                }
            }
        }
    }
}

fn get_keyboard_keys(pins: &[&dyn InputPin<Error = Infallible>; 3]) -> [Keyboard; 3] {
    [
        if pins[0].is_low().unwrap() {
            Keyboard::A
        } else {
            Keyboard::NoEventIndicated
        },
        if pins[1].is_low().unwrap() {
            Keyboard::B
        } else {
            Keyboard::NoEventIndicated
        },
        if pins[2].is_low().unwrap() {
            Keyboard::C
        } else {
            Keyboard::NoEventIndicated
        },
    ]
}

fn get_consumer_codes(pins: &[&dyn InputPin<Error = Infallible>; 2]) -> [Consumer; 2] {
    [
        if pins[0].is_low().unwrap() {
            Consumer::VolumeDecrement
        } else {
            Consumer::Unassigned
        },
        if pins[1].is_low().unwrap() {
            Consumer::VolumeIncrement
        } else {
            Consumer::Unassigned
        },
    ]
}

fn update_mouse_report(
    mut report: WheelMouseReport,
    pins: &[&dyn InputPin<Error = core::convert::Infallible>; 7],
) -> WheelMouseReport {
    if pins[0].is_low().unwrap() {
        report.buttons |= 0x1; //Left
    } else {
        report.buttons &= 0xFF - 0x1;
    }
    if pins[1].is_low().unwrap() {
        report.buttons |= 0x4; //Middle
    } else {
        report.buttons &= 0xFF - 0x4;
    }
    if pins[2].is_low().unwrap() {
        report.buttons |= 0x2; //Right
    } else {
        report.buttons &= 0xFF - 0x2;
    }
    if pins[3].is_low().unwrap() {
        report.y = i8::saturating_add(report.y, -10); //Up
    }
    if pins[4].is_low().unwrap() {
        report.x = i8::saturating_add(report.x, -10); //Left
    }
    if pins[5].is_low().unwrap() {
        report.y = i8::saturating_add(report.y, 10); //Down
    }
    if pins[6].is_low().unwrap() {
        report.x = i8::saturating_add(report.x, 10); //Right
    }

    report
}
