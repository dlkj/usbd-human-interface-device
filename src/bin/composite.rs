#![no_std]
#![no_main]

use adafruit_macropad::hal;
use core::cell::Cell;
use core::default::Default;
use cortex_m::interrupt::Mutex;
use cortex_m_rt::entry;
use embedded_hal::digital::v2::*;
use embedded_hal::prelude::_embedded_hal_timer_CountDown;
use embedded_time::duration::{Extensions, Milliseconds};
use embedded_time::rate::Hertz;
use hal::pac;
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

static USB_DEVICES: Mutex<Cell<Option<UsbDevices>>> = Mutex::new(Cell::new(None));

#[derive(Default, Eq, PartialEq)]
struct UsbData {
    keyboard_report: Option<[u8; 8]>,
    mouse_report: Option<[u8; 3]>,
    consumer_report: Option<[u8; 8]>,
    keyboard_led_report: Option<[u8; 1]>,
}

static USB_DATA: Mutex<Cell<UsbData>> = Mutex::new(Cell::new(UsbData {
    keyboard_report: None,
    mouse_report: None,
    consumer_report: None,
    keyboard_led_report: None,
}));

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

    init_display(oled_spi, oled_dc.into(), oled_cs.into());
    check_for_persisted_panic(&button);
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
        .new_interface(BOOT_KEYBOARD_REPORT_DESCRIPTOR)
        .boot_device(InterfaceProtocol::Keyboard)
        .description("Keyboard")
        .idle_default(Milliseconds(500))
        .unwrap()
        .in_endpoint(UsbPacketSize::Size8, Milliseconds(20))
        .unwrap()
        .without_out_endpoint()
        .build_interface()
        .unwrap()
        //Boot Mouse - interface 1
        .new_interface(BOOT_MOUSE_REPORT_DESCRIPTOR)
        .boot_device(InterfaceProtocol::Mouse)
        .description("Mouse")
        .idle_default(Milliseconds(0))
        .unwrap()
        .in_endpoint(UsbPacketSize::Size8, Milliseconds(20))
        .unwrap()
        .without_out_endpoint()
        .build_interface()
        .unwrap()
        //Consumer control - interface 2
        .new_interface(MULTIPLE_CODE_REPORT_DESCRIPTOR)
        .description("Consumer Control")
        .idle_default(Milliseconds(0))
        .unwrap()
        .in_endpoint(UsbPacketSize::Size8, Milliseconds(20))
        .unwrap()
        .without_out_endpoint()
        .build_interface()
        .unwrap()
        //Build
        .build()
        .unwrap();

    //https://pid.codes
    let usb_dev = UsbDeviceBuilder::new(usb_alloc, UsbVidPid(0x1209, 0x0001))
        .manufacturer("DLKJ")
        .product("Keyboard & Mouse")
        .serial_number("TEST")
        .device_class(3) // HID - from: https://www.usb.org/defined-class-codes
        .composite_with_iads()
        .supports_remote_wakeup(false)
        .build();

    cortex_m::interrupt::free(|cs| {
        USB_DEVICES.borrow(cs).replace(Some((usb_dev, composite)));
    });

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

    let mut count_down = timer.count_down();

    // Enable the USB interrupt
    unsafe {
        pac::NVIC::unmask(hal::pac::Interrupt::USBCTRL_IRQ);
    };

    loop {
        if button.is_low().unwrap() {
            hal::rom_data::reset_to_usb_boot(0x1 << 13, 0x0);
        }

        let mut buttons: u8 = 0;
        let mut x = 0;
        let mut y = 0;

        if in0.is_low().unwrap() {
            buttons |= 0x1; //Left
        }
        if in1.is_low().unwrap() {
            buttons |= 0x4; //Middle
        }
        if in2.is_low().unwrap() {
            buttons |= 0x2; //Right
        }
        if in4.is_low().unwrap() {
            y += -5; //Up
        }
        if in6.is_low().unwrap() {
            x += -5; //Left
        }
        if in7.is_low().unwrap() {
            y += 5; //Down
        }
        if in8.is_low().unwrap() {
            x += 5; //Right
        }

        let consumer_codes = [
            if in3.is_low().unwrap() {
                Consumer::VolumeDecrement
            } else {
                Consumer::Unassigned
            },
            if in5.is_low().unwrap() {
                Consumer::VolumeIncrement
            } else {
                Consumer::Unassigned
            },
        ];

        let keys = [
            if in9.is_low().unwrap() {
                Keyboard::A
            } else {
                Keyboard::NoEventIndicated
            },
            if in10.is_low().unwrap() {
                Keyboard::B
            } else {
                Keyboard::NoEventIndicated
            },
            if in11.is_low().unwrap() {
                Keyboard::C
            } else {
                Keyboard::NoEventIndicated
            },
        ];

        let usb_data = UsbData {
            keyboard_report: Some(
                BootKeyboardReport::new(keys)
                    .pack()
                    .expect("Failed to pack keyboard report"),
            ),
            mouse_report: Some(
                BootMouseReport { buttons, x, y }
                    .pack()
                    .expect("Failed to pack mouse report"),
            ),
            consumer_report: Some(
                MultipleConsumerReport {
                    codes: [
                        consumer_codes[0],
                        consumer_codes[1],
                        Consumer::Unassigned,
                        Consumer::Unassigned,
                    ],
                }
                .pack()
                .expect("Failed to pack consumer report"),
            ),
            keyboard_led_report: None,
        };

        let keyboard_leds = cortex_m::interrupt::free(|cs| {
            let data = USB_DATA.borrow(cs).replace(usb_data);
            data.keyboard_led_report
        });

        if let Some(leds) = keyboard_leds
            .map(|l| KeyboardLeds::unpack(&l).expect("Failed to parse keyboard led report"))
        {
            led_pin.set_state(PinState::from(leds.num_lock)).ok();
        }

        count_down.start(10.milliseconds());
        nb::block!(count_down.wait()).unwrap();
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
            let mut new_data: UsbData = Default::default();

            let mut buf = [1];
            new_data.keyboard_led_report = match composite
                .get_interface_mut(0)
                .unwrap()
                .read_report(&mut buf)
            {
                Err(UsbError::WouldBlock) => None,
                Err(e) => {
                    panic!("Failed to read keyboard report: {:?}", e)
                }
                Ok(_) => Some(buf),
            };

            let data = cortex_m::interrupt::free(|cs| USB_DATA.borrow(cs).replace(new_data));

            if let Some(k) = data.keyboard_report {
                match composite.get_interface_mut(0).unwrap().write_report(&k) {
                    Err(UsbError::WouldBlock) => {}
                    Ok(_) => {}
                    Err(e) => {
                        panic!("Failed to write keyboard report: {:?}", e)
                    }
                };
            }
            if let Some(m) = data.mouse_report {
                match composite.get_interface_mut(1).unwrap().write_report(&m) {
                    Err(UsbError::WouldBlock) => {}
                    Ok(_) => {}
                    Err(e) => {
                        panic!("Failed to write mouse report: {:?}", e)
                    }
                }
            }
            if let Some(c) = data.consumer_report {
                match composite.get_interface_mut(2).unwrap().write_report(&c) {
                    Err(UsbError::WouldBlock) => {}
                    Ok(_) => {}
                    Err(e) => {
                        panic!("Failed to write consumer control report: {:?}", e)
                    }
                }
            }
        }
    }
}
