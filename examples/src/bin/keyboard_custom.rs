#![no_std]
#![no_main]

use core::convert::Infallible;

use bsp::entry;
use bsp::hal;
use defmt::*;
use defmt_rtt as _;
use embedded_hal::digital::v2::*;
use embedded_hal::prelude::*;
use fugit::ExtU32;
use fugit::MillisDurationU32;
use hal::pac;
use hal::timer::CountDown;
use packed_struct::prelude::*;
use panic_probe as _;
#[allow(clippy::wildcard_imports)]
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_human_interface_device::device::keyboard::{BootKeyboardReport, KeyboardLedsReport};
use usbd_human_interface_device::interface::{InBytes8, OutBytes8, ReportSingle};
use usbd_human_interface_device::page::Keyboard;
use usbd_human_interface_device::usb_class::prelude::*;

use rp_pico as bsp;

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
    let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
        pac.USBCTRL_REGS,
        pac.USBCTRL_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));

    // Descriptor based on the Logitech Gaming Keyboard
    // http://www.usblyzer.com/reports/usb-properties/usb-keyboard.html
    #[rustfmt::skip]
    pub const LOGITECH_GAMING_KEYBOARD_REPORT_DESCRIPTOR: &[u8] = &[
        0x05, 0x01, //    Usage Page (Generic Desktop)
        0x09, 0x06, //    Usage (Keyboard)
        0xA1, 0x01, //    Collection (Application)
        0x05, 0x07, //        Usage Page (Keyboard/Keypad)
        0x19, 0xE0, //        Usage Minimum (Keyboard Left Control)
        0x29, 0xE7, //        Usage Maximum (Keyboard Right GUI)
        0x15, 0x00, //        Logical Minimum (0)
        0x25, 0x01, //        Logical Maximum (1)
        0x75, 0x01, //        Report Size (1)
        0x95, 0x08, //        Report Count (8)
        0x81, 0x02, //        Input (Data,Var,Abs,NWrp,Lin,Pref,NNul,Bit)
        0x95, 0x01, //        Report Count (1)
        0x75, 0x08, //        Report Size (8)
        0x81, 0x01, //        Input (Const,Ary,Abs)
        0x95, 0x05, //        Report Count (5)
        0x75, 0x01, //        Report Size (1)
        0x05, 0x08, //        Usage Page (LEDs)
        0x19, 0x01, //        Usage Minimum (Num Lock)
        0x29, 0x05, //        Usage Maximum (Kana)
        0x91, 0x02, //        Output (Data,Var,Abs,NWrp,Lin,Pref,NNul,NVol,Bit)
        0x95, 0x01, //        Report Count (1)
        0x75, 0x03, //        Report Size (3)
        0x91, 0x01, //        Output (Const,Ary,Abs,NWrp,Lin,Pref,NNul,NVol,Bit)
        0x95, 0x06, //        Report Count (6)
        0x75, 0x08, //        Report Size (8)
        0x15, 0x00, //        Logical Minimum (0)
        0x26, 0x97, 0x00, //        Logical Maximum (151)
        0x05, 0x07, //        Usage Page (Keyboard/Keypad)
        0x19, 0x00, //        Usage Minimum (Undefined)
        0x29, 0x97, //        Usage Maximum (Keyboard LANG8)
        0x81, 0x00, //        Input (Data,Ary,Abs)
        0xC0, //        End Collection
    ];

    let mut keyboard = UsbHidClassBuilder::new()
        .add_device(
            InterfaceBuilder::<InBytes8, OutBytes8, ReportSingle>::new(
                LOGITECH_GAMING_KEYBOARD_REPORT_DESCRIPTOR,
            )
            .unwrap()
            .description("Custom Keyboard")
            .idle_default(500.millis())
            .unwrap()
            .in_endpoint(10.millis())
            .unwrap()
            .with_out_endpoint(100.millis())
            .unwrap()
            .build(),
        )
        .build(&usb_bus);

    //https://pid.codes
    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x1209, 0x0001))
        .manufacturer("usbd-human-interface-device")
        .product("Custom Keyboard")
        .serial_number("TEST")
        .build();

    //GPIO pins
    let mut led_pin = pins.gpio13.into_push_pull_output();

    let keys: &[&dyn InputPin<Error = core::convert::Infallible>] = &[
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

    led_pin.set_low().ok();

    let mut last_keys = None;

    let mut input_count_down = timer.count_down();
    input_count_down.start(10.millis());

    let mut idle_count_down = reset_idle(&timer, keyboard.device().global_idle());

    loop {
        //Poll the keys every 10ms
        if input_count_down.wait().is_ok() {
            if idle_count_down
                .as_mut()
                .map(|c| c.wait().is_ok())
                .unwrap_or(false)
            {
                //Expire on idle
                last_keys = None;
            }

            let keys = get_keys(keys);

            if last_keys.map(|k| k != keys).unwrap_or(true) {
                match keyboard
                    .device()
                    .write_report(&BootKeyboardReport::new(keys).pack().unwrap())
                {
                    Err(UsbError::WouldBlock) => {}
                    Ok(_) => {
                        last_keys = Some(keys);
                        idle_count_down = reset_idle(&timer, keyboard.device().global_idle());
                    }
                    Err(e) => {
                        core::panic!("Failed to write keyboard report: {:?}", e)
                    }
                };
            }
        }

        if usb_dev.poll(&mut [&mut keyboard]) {
            let data = &mut [0];
            match keyboard.device().read_report(data) {
                Err(UsbError::WouldBlock) => {
                    //do nothing
                }
                Err(e) => {
                    core::panic!("Failed to read keyboard report: {:?}", e)
                }
                Ok(_) => {
                    //send scroll lock to the led
                    led_pin
                        .set_state(PinState::from(
                            KeyboardLedsReport::unpack(data).unwrap().num_lock,
                        ))
                        .ok();
                }
            }
        }
    }
}

fn reset_idle(timer: &hal::Timer, idle: MillisDurationU32) -> Option<CountDown> {
    if idle.ticks() == 0 {
        None
    } else {
        let mut count_down = timer.count_down();
        count_down.start(idle.convert());
        Some(count_down)
    }
}

fn get_keys(keys: &[&dyn InputPin<Error = Infallible>]) -> [Keyboard; 12] {
    [
        if keys[0].is_low().unwrap() {
            Keyboard::KeypadNumLockAndClear
        } else {
            Keyboard::NoEventIndicated
        }, //Numlock
        if keys[1].is_low().unwrap() {
            Keyboard::UpArrow
        } else {
            Keyboard::NoEventIndicated
        }, //Up
        if keys[2].is_low().unwrap() {
            Keyboard::F12
        } else {
            Keyboard::NoEventIndicated
        }, //F12
        if keys[3].is_low().unwrap() {
            Keyboard::LeftArrow
        } else {
            Keyboard::NoEventIndicated
        }, //Left
        if keys[4].is_low().unwrap() {
            Keyboard::DownArrow
        } else {
            Keyboard::NoEventIndicated
        }, //Down
        if keys[5].is_low().unwrap() {
            Keyboard::RightArrow
        } else {
            Keyboard::NoEventIndicated
        }, //Right
        if keys[6].is_low().unwrap() {
            Keyboard::A
        } else {
            Keyboard::NoEventIndicated
        }, //A
        if keys[7].is_low().unwrap() {
            Keyboard::B
        } else {
            Keyboard::NoEventIndicated
        }, //B
        if keys[8].is_low().unwrap() {
            Keyboard::C
        } else {
            Keyboard::NoEventIndicated
        }, //C
        if keys[9].is_low().unwrap() {
            Keyboard::LeftControl
        } else {
            Keyboard::NoEventIndicated
        }, //LCtrl
        if keys[10].is_low().unwrap() {
            Keyboard::LeftShift
        } else {
            Keyboard::NoEventIndicated
        }, //LShift
        if keys[11].is_low().unwrap() {
            Keyboard::ReturnEnter
        } else {
            Keyboard::NoEventIndicated
        }, //Enter
    ]
}
