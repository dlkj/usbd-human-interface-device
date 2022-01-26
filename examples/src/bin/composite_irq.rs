#![no_std]
#![no_main]

use core::cell::RefCell;
use core::convert::Infallible;
use core::default::Default;

use adafruit_macropad::hal;
use cortex_m::interrupt::Mutex;
use cortex_m_rt::entry;
use embedded_hal::digital::v2::*;
use embedded_hal::prelude::_embedded_hal_timer_CountDown;
use embedded_time::duration::Milliseconds;
use embedded_time::rate::Hertz;
use hal::pac;
use hal::timer::CountDown;
use hal::Clock;
use log::*;
use pac::interrupt;
use packed_struct::prelude::*;
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_hid_devices::device::consumer::{MultipleConsumerReport, MULTIPLE_CODE_REPORT_DESCRIPTOR};
use usbd_hid_devices::device::keyboard::{
    BootKeyboardReport, KeyboardLeds, BOOT_KEYBOARD_REPORT_DESCRIPTOR,
};
use usbd_hid_devices::device::mouse::{BootMouseReport, BOOT_MOUSE_REPORT_DESCRIPTOR};
use usbd_hid_devices::hid_class::prelude::*;
use usbd_hid_devices::page::Consumer;
use usbd_hid_devices::page::Keyboard;

use usbd_hid_devices_example_rp2040::*;

type UsbDevices = (
    UsbDevice<'static, hal::usb::UsbBus>,
    UsbHidClass<'static, hal::usb::UsbBus>,
);

static USB_DEVICES: Mutex<RefCell<Option<UsbDevices>>> = Mutex::new(RefCell::new(None));
static KEYBOARD_REPORT: Mutex<RefCell<Option<BootKeyboardReport>>> = Mutex::new(RefCell::new(None));
static MOUSE_REPORT: Mutex<RefCell<Option<BootMouseReport>>> = Mutex::new(RefCell::new(None));
static CONSUMER_REPORT: Mutex<RefCell<Option<MultipleConsumerReport>>> =
    Mutex::new(RefCell::new(None));
static KEYBOARD_STATUS: Mutex<RefCell<(KeyboardLeds, Milliseconds, UsbDeviceState)>> =
    Mutex::new(RefCell::new((
        KeyboardLeds {
            num_lock: false,
            caps_lock: false,
            scroll_lock: false,
            compose: false,
            kana: false,
        },
        Milliseconds(500),
        UsbDeviceState::Default,
    )));

const DEFAULT_KEYBOARD_IDLE: Milliseconds = Milliseconds(500);
const KEYBOARD_MOUSE_POLL: Milliseconds = Milliseconds(10);
const KEYBOARD_LED_POLL: Milliseconds = Milliseconds(100);
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

    let composite = UsbHidClassBuilder::new(usb_alloc)
        //Boot Keyboard - interface 0
        .new_interface(
            UsbHidInterfaceBuilder::new(BOOT_KEYBOARD_REPORT_DESCRIPTOR)
                .boot_device(InterfaceProtocol::Keyboard)
                .description("Keyboard")
                .idle_default(DEFAULT_KEYBOARD_IDLE)
                .unwrap()
                .in_endpoint(UsbPacketSize::Size8, KEYBOARD_MOUSE_POLL)
                .unwrap()
                .with_out_endpoint(UsbPacketSize::Size8, KEYBOARD_LED_POLL)
                .unwrap()
                .build_interface(),
        )
        .unwrap()
        //Boot Mouse - interface 1
        .new_interface(
            UsbHidInterfaceBuilder::new(BOOT_MOUSE_REPORT_DESCRIPTOR)
                .boot_device(InterfaceProtocol::Mouse)
                .description("Mouse")
                .idle_default(Milliseconds(0))
                .unwrap()
                .in_endpoint(UsbPacketSize::Size8, KEYBOARD_MOUSE_POLL)
                .unwrap()
                .without_out_endpoint()
                .build_interface(),
        )
        .unwrap()
        //Consumer control - interface 2
        .new_interface(
            UsbHidInterfaceBuilder::new(MULTIPLE_CODE_REPORT_DESCRIPTOR)
                .description("Consumer Control")
                .idle_default(Milliseconds(0))
                .unwrap()
                .in_endpoint(UsbPacketSize::Size8, CONSUMER_POLL)
                .unwrap()
                .without_out_endpoint()
                .build_interface(),
        )
        .unwrap()
        //Build
        .build()
        .unwrap();

    //https://pid.codes
    let usb_dev = UsbDeviceBuilder::new(usb_alloc, UsbVidPid(0x1209, 0x0001))
        .manufacturer("usbd-hid-devices")
        .product("Keyboard, Mouse & Consumer")
        .serial_number("TEST")
        .supports_remote_wakeup(false)
        .build();

    cortex_m::interrupt::free(|cs| {
        USB_DEVICES.borrow(cs).replace(Some((usb_dev, composite)));
    });
    let mut led_pin = pins.gpio13.into_push_pull_output();
    led_pin.set_low().ok();

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

    let mut keyboard_mouse_poll = timer.count_down();
    keyboard_mouse_poll.start(KEYBOARD_MOUSE_POLL);
    let mut last_keyboard_report = None;
    let mut keyboard_idle = reset_idle(&timer, DEFAULT_KEYBOARD_IDLE);
    let mut last_mouse_buttons = 0;
    let mut mouse_report = BootMouseReport::default();

    let mut consumer_poll = timer.count_down();
    consumer_poll.start(CONSUMER_POLL);
    let mut last_consumer_report = MultipleConsumerReport::default();

    let mut write_pending_poll = timer.count_down();
    write_pending_poll.start(WRITE_PENDING_POLL);

    let mut display_poll = timer.count_down();
    display_poll.start(DISPLAY_POLL);

    let mut state = UsbDeviceState::Default;

    // Enable the USB interrupt
    unsafe {
        pac::NVIC::unmask(hal::pac::Interrupt::USBCTRL_IRQ);
    };

    loop {
        if button.is_low().unwrap() {
            hal::rom_data::reset_to_usb_boot(0x1 << 13, 0x0);
        }

        if keyboard_mouse_poll.wait().is_ok() {
            if keyboard_idle
                .as_mut()
                .map(|c| c.wait().is_ok())
                .unwrap_or(false)
            {
                //Expire on idle
                last_keyboard_report = None;
            }

            let (leds, idle, usb_state) =
                cortex_m::interrupt::free(|cs| *KEYBOARD_STATUS.borrow(cs).borrow());
            led_pin.set_state(PinState::from(leds.num_lock)).ok();
            state = usb_state;

            if usb_state == UsbDeviceState::Configured {
                let keyboard_report = BootKeyboardReport::new(get_keyboard_keys(keys));
                if last_keyboard_report
                    .map(|r| r != keyboard_report)
                    .unwrap_or(true)
                {
                    cortex_m::interrupt::free(|cs| {
                        let mut kr = KEYBOARD_REPORT.borrow(cs).borrow_mut();
                        if kr.is_none() {
                            kr.replace(keyboard_report);
                            last_keyboard_report = Some(keyboard_report);
                            keyboard_idle = reset_idle(&timer, idle);
                        }
                    });
                }

                mouse_report = update_mouse_report(mouse_report, keys);
                if mouse_report.buttons != last_mouse_buttons
                    || mouse_report.x != 0
                    || mouse_report.y != 0
                {
                    cortex_m::interrupt::free(|cs| {
                        let mut mr = MOUSE_REPORT.borrow(cs).borrow_mut();
                        if mr.is_none() {
                            mr.replace(mouse_report);
                            last_mouse_buttons = mouse_report.buttons;
                            mouse_report = Default::default();
                        }
                    });
                }
            }
        }

        if state == UsbDeviceState::Configured && consumer_poll.wait().is_ok() {
            let codes = get_consumer_codes(keys);
            let consumer_report = MultipleConsumerReport {
                codes: [
                    codes[0],
                    codes[1],
                    Consumer::Unassigned,
                    Consumer::Unassigned,
                ],
            };

            if last_consumer_report != consumer_report {
                cortex_m::interrupt::free(|cs| {
                    let mut cr = CONSUMER_REPORT.borrow(cs).borrow_mut();
                    if cr.is_none() {
                        cr.replace(consumer_report);
                        last_consumer_report = consumer_report;
                    }
                });
            }
        }

        //pend the usb interrupt if there is data ready to write
        if write_pending_poll.wait().is_ok() {
            let write_pending = cortex_m::interrupt::free(|cs| {
                KEYBOARD_REPORT.borrow(cs).borrow().is_some()
                    || MOUSE_REPORT.borrow(cs).borrow().is_some()
                    || CONSUMER_REPORT.borrow(cs).borrow().is_some()
            });

            if write_pending {
                pac::NVIC::pend(pac::interrupt::USBCTRL_IRQ);
                cortex_m::asm::isb();
            }
        }

        if display_poll.wait().is_ok() {
            log::logger().flush();
        }
    }
}

//noinspection Annotator
#[allow(non_snake_case)]
#[interrupt]
fn USBCTRL_IRQ() {
    static mut IRQ_USB_DEVICES: Option<UsbDevices> = None;

    if IRQ_USB_DEVICES.is_none() {
        cortex_m::interrupt::free(|cs| {
            *IRQ_USB_DEVICES = USB_DEVICES.borrow(cs).take();
        });
    }

    if let Some((ref mut usb_device, ref mut composite)) = IRQ_USB_DEVICES {
        if usb_device.poll(&mut [composite]) {
            let mut buf = [1];
            match composite
                .get_interface_mut(0)
                .unwrap()
                .read_report(&mut buf)
            {
                Err(UsbError::WouldBlock) => {}
                Err(e) => {
                    panic!("Failed to read keyboard report: {:?}", e)
                }
                Ok(_) => {
                    let leds = KeyboardLeds::unpack(&buf).expect("Failed to unpack Keyboard Leds");
                    let idle = composite.get_interface_mut(0).unwrap().global_idle();

                    cortex_m::interrupt::free(|cs| {
                        KEYBOARD_STATUS
                            .borrow(cs)
                            .replace((leds, idle, usb_device.state()))
                    });
                }
            }
        }

        if usb_device.state() == UsbDeviceState::Configured {
            cortex_m::interrupt::free(|cs| {
                let keyboard_report = *KEYBOARD_REPORT.borrow(cs).borrow();
                let report = keyboard_report;
                if let Some(r) = report {
                    match composite
                        .get_interface_mut(0)
                        .unwrap()
                        .write_report(&r.pack().expect("Failed to pack keyboard report"))
                    {
                        Err(UsbError::WouldBlock) => {}
                        Ok(_) => {
                            KEYBOARD_REPORT.borrow(cs).replace(None);
                        }
                        Err(e) => {
                            panic!("Failed to write keyboard report: {:?}", e)
                        }
                    };
                }

                let mouse_report_ref = *MOUSE_REPORT.borrow(cs).borrow();
                if let Some(r) = mouse_report_ref {
                    match composite
                        .get_interface_mut(1)
                        .unwrap()
                        .write_report(&r.pack().expect("Failed to pack mouse report"))
                    {
                        Err(UsbError::WouldBlock) => {}
                        Ok(_) => {
                            MOUSE_REPORT.borrow(cs).replace(None);
                        }
                        Err(e) => {
                            panic!("Failed to write mouse report: {:?}", e)
                        }
                    };
                }

                let consumer_report = *CONSUMER_REPORT.borrow(cs).borrow();
                if let Some(r) = consumer_report {
                    match composite
                        .get_interface_mut(2)
                        .unwrap()
                        .write_report(&r.pack().expect("Failed to pack consumer report"))
                    {
                        Err(UsbError::WouldBlock) => {}
                        Ok(_) => {
                            CONSUMER_REPORT.borrow(cs).replace(None);
                        }
                        Err(e) => {
                            panic!("Failed to write consumer report: {:?}", e)
                        }
                    };
                }
            });
        }
    }

    cortex_m::asm::sev();
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
    mut report: BootMouseReport,
    keys: &[&dyn InputPin<Error = core::convert::Infallible>],
) -> BootMouseReport {
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

fn reset_idle(timer: &hal::Timer, idle: Milliseconds) -> Option<CountDown> {
    if idle <= Milliseconds(0_u32) {
        None
    } else {
        let mut count_down = timer.count_down();
        count_down.start(idle);
        Some(count_down)
    }
}
