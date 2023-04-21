#![no_std]
#![no_main]

use core::cell::{Cell, RefCell};
use core::convert::Infallible;
use core::default::Default;

use bsp::entry;
use bsp::hal;
use cortex_m::interrupt::Mutex;
use cortex_m::prelude::*;
use defmt::*;
use defmt_rtt as _;
use embedded_hal::digital::v2::*;
use frunk::HList;
use fugit::{ExtU32, MicrosDurationU32};
use hal::gpio::bank0::*;
use hal::gpio::{Output, Pin, PushPull};
use hal::pac;
use pac::interrupt;
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

type UsbDevices = (
    UsbDevice<'static, hal::usb::UsbBus>,
    UsbHidClass<
        'static,
        hal::usb::UsbBus,
        HList!(
            ConsumerControl<'static, hal::usb::UsbBus>,
            WheelMouse<'static, hal::usb::UsbBus>,
            NKROBootKeyboard<'static, hal::usb::UsbBus>,
        ),
    >,
);
type LedPin = Pin<Gpio13, Output<PushPull>>;

static IRQ_SHARED: Mutex<RefCell<Option<UsbDevices>>> = Mutex::new(RefCell::new(None));
static USBCTRL: Mutex<Cell<Option<LedPin>>> = Mutex::new(Cell::new(None));

const KEYBOARD_MOUSE_POLL: MicrosDurationU32 = MicrosDurationU32::millis(10);
const CONSUMER_POLL: MicrosDurationU32 = MicrosDurationU32::millis(50);

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

    let multi_device = UsbHidClassBuilder::new()
        .add_device(
            usbd_human_interface_device::device::keyboard::NKROBootKeyboardConfig::default(),
        )
        .add_device(usbd_human_interface_device::device::mouse::WheelMouseConfig::default())
        .add_device(usbd_human_interface_device::device::consumer::ConsumerControlConfig::default())
        //Build
        .build(usb_alloc);

    //https://pid.codes
    let usb_dev = UsbDeviceBuilder::new(usb_alloc, UsbVidPid(0x1209, 0x0001))
        .manufacturer("usbd-human-interface-device")
        .product("Keyboard, Mouse & Consumer")
        .serial_number("TEST")
        .build();

    cortex_m::interrupt::free(|cs| {
        IRQ_SHARED.borrow(cs).replace(Some((usb_dev, multi_device)));
        USBCTRL
            .borrow(cs)
            .replace(Some(pins.gpio13.into_push_pull_output()));
    });

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

    let mut keyboard_mouse_input_timer = timer.count_down();
    keyboard_mouse_input_timer.start(KEYBOARD_MOUSE_POLL);

    let mut last_mouse_buttons = 0;
    let mut mouse_report = WheelMouseReport::default();

    let mut consumer_input_timer = timer.count_down();
    consumer_input_timer.start(CONSUMER_POLL);
    let mut last_consumer_report = MultipleConsumerReport::default();

    let mut tick_timer = timer.count_down();
    tick_timer.start(1.millis());

    // Enable the USB interrupt
    unsafe {
        pac::NVIC::unmask(hal::pac::Interrupt::USBCTRL_IRQ);
    };

    loop {
        if keyboard_mouse_input_timer.wait().is_ok() {
            cortex_m::interrupt::free(|cs| {
                let mut usb_ref = IRQ_SHARED.borrow(cs).borrow_mut();
                let (_, ref mut multi_device) = usb_ref.as_mut().unwrap();

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
                            mouse_report = WheelMouseReport::default();
                        }
                        Err(e) => {
                            core::panic!("Failed to write mouse report: {:?}", e)
                        }
                    };
                }
            });
        }

        if consumer_input_timer.wait().is_ok() {
            cortex_m::interrupt::free(|cs| {
                let mut usb_ref = IRQ_SHARED.borrow(cs).borrow_mut();
                let (_, ref mut multi_device) = usb_ref.as_mut().unwrap();

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
            });
        }

        //Tick once per ms
        if tick_timer.wait().is_ok() {
            cortex_m::interrupt::free(|cs| {
                let mut usb_ref = IRQ_SHARED.borrow(cs).borrow_mut();
                let (_, ref mut multi_device) = usb_ref.as_mut().unwrap();

                //Process any managed functionality
                match multi_device.tick() {
                    Err(UsbHidError::WouldBlock) => {}
                    Ok(_) => {}
                    Err(e) => {
                        core::panic!("Failed to process keyboard tick: {:?}", e)
                    }
                };
            });
        }
    }
}

//noinspection Annotator
#[allow(non_snake_case)]
#[interrupt]
fn USBCTRL_IRQ() {
    static mut LED_PIN: Option<LedPin> = None;
    if LED_PIN.is_none() {
        cortex_m::interrupt::free(|cs| {
            *LED_PIN = USBCTRL.borrow(cs).take();
        });
    }

    cortex_m::interrupt::free(|cs| {
        let mut usb_ref = IRQ_SHARED.borrow(cs).borrow_mut();
        if usb_ref.is_none() {
            return;
        }

        let (ref mut usb_device, ref mut multi_device) = usb_ref.as_mut().unwrap();
        if usb_device.poll(&mut [multi_device]) {
            let keyboard = multi_device.device::<NKROBootKeyboard<'_, _>, _>();
            match keyboard.read_report() {
                Err(UsbError::WouldBlock) => {}
                Err(e) => {
                    core::panic!("Failed to read keyboard report: {:?}", e)
                }
                Ok(leds) => {
                    LED_PIN
                        .as_mut()
                        .map(|p| p.set_state(PinState::from(leds.num_lock)).ok());
                }
            }
        }
    });

    cortex_m::asm::sev();
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
