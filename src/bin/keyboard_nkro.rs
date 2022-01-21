#![no_std]
#![no_main]

use adafruit_macropad::hal;
use cortex_m_rt::entry;
use embedded_hal::digital::v2::*;
use embedded_time::duration::Milliseconds;
use embedded_time::rate::Hertz;
use hal::pac;
use hal::Clock;
use log::*;
use packed_struct::prelude::*;
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_hid_devices::device::keyboard::KeyboardLeds;
use usbd_hid_devices::hid_class::prelude::*;
use usbd_hid_devices::page::Keyboard;
use usbd_hid_devices_example_rp2040::*;

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

    let button = pins.gpio0.into_pull_up_input();

    init_display(oled_spi, oled_dc.into(), oled_cs.into());
    check_for_persisted_panic(&button);
    info!("Starting up...");

    //USB
    let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
        pac.USBCTRL_REGS,
        pac.USBCTRL_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));

    //25 bytes
    //byte 0 - modifiers
    //byte 1 - reserved 0s
    //byte 2-7 - array of keycodes - used for boot support
    //byte 9-24 - bit array of pressed keys
    #[rustfmt::skip]
    pub const NKRO_BOOT_KEYBOARD_REPORT_DESCRIPTOR: &[u8] = &[
        0x05, 0x01,                     // Usage Page (Generic Desktop),
        0x09, 0x06,                     // Usage (Keyboard),
        0xA1, 0x01,                     // Collection (Application),
        // bitmap of modifiers
        0x75, 0x01,                     //   Report Size (1),
        0x95, 0x08,                     //   Report Count (8),
        0x05, 0x07,                     //   Usage Page (Key Codes),
        0x19, 0xE0,                     //   Usage Minimum (224),
        0x29, 0xE7,                     //   Usage Maximum (231),
        0x15, 0x00,                     //   Logical Minimum (0),
        0x25, 0x01,                     //   Logical Maximum (1),
        0x81, 0x02,                     //   Input (Data, Variable, Absolute), ;Modifier byte
        // 7 bytes of padding
        0x75, 0x38,                     //   Report Size (0x38),
        0x95, 0x01,                     //   Report Count (1),
        0x81, 0x01,                     //   Input (Constant), ;Reserved byte
        // LED output report
        0x95, 0x05,                     //   Report Count (5),
        0x75, 0x01,                     //   Report Size (1),
        0x05, 0x08,                     //   Usage Page (LEDs),
        0x19, 0x01,                     //   Usage Minimum (1),
        0x29, 0x05,                     //   Usage Maximum (5),
        0x91, 0x02,                     //   Output (Data, Variable, Absolute),
        0x95, 0x01,                     //   Report Count (1),
        0x75, 0x03,                     //   Report Size (3),
        0x91, 0x03,                     //   Output (Constant),
        // bitmap of keys
        0x95, 0x88,                     //   Report Count () - (REPORT_BYTES-1)*8
        0x75, 0x01,                     //   Report Size (1),
        0x15, 0x00,                     //   Logical Minimum (0),
        0x25, 0x01,                     //   Logical Maximum(1),
        0x05, 0x07,                     //   Usage Page (Key Codes),
        0x19, 0x00,                     //   Usage Minimum (0),
        0x29, 0x87,                     //   Usage Maximum (), - (REPORT_BYTES-1)*8-1
        0x81, 0x02,                     //   Input (Data, Variable, Absolute),
        0xc0                            // End Collection
    ];

    //18 bytes - derived from https://learn.adafruit.com/custom-hid-devices-in-circuitpython/n-key-rollover-nkro-hid-device
    //First byte modifiers, 17 byte key bit array
    #[rustfmt::skip]
    pub const _NKRO_COMPACT_KEYBOARD_REPORT_DESCRIPTOR: &[u8] = &[
        0x05, 0x01,                     // Usage Page (Generic Desktop),
        0x09, 0x06,                     // Usage (Keyboard),
        0xA1, 0x01,                     // Collection (Application),
        // bitmap of modifiers
        0x75, 0x01,                     //   Report Size (1),
        0x95, 0x08,                     //   Report Count (8),
        0x05, 0x07,                     //   Usage Page (Key Codes),
        0x19, 0xE0,                     //   Usage Minimum (224),
        0x29, 0xE7,                     //   Usage Maximum (231),
        0x15, 0x00,                     //   Logical Minimum (0),
        0x25, 0x01,                     //   Logical Maximum (1),
        0x81, 0x02,                     //   Input (Data, Variable, Absolute), ;Modifier byte
        // LED output report
        0x95, 0x05,                     //   Report Count (5),
        0x75, 0x01,                     //   Report Size (1),
        0x05, 0x08,                     //   Usage Page (LEDs),
        0x19, 0x01,                     //   Usage Minimum (1),
        0x29, 0x05,                     //   Usage Maximum (5),
        0x91, 0x02,                     //   Output (Data, Variable, Absolute),
        0x95, 0x01,                     //   Report Count (1),
        0x75, 0x03,                     //   Report Size (3),
        0x91, 0x03,                     //   Output (Constant),
        // bitmap of keys
        0x95, 0x88,                     //   Report Count () - (REPORT_BYTES-1)*8
        0x75, 0x01,                     //   Report Size (1),
        0x15, 0x00,                     //   Logical Minimum (0),
        0x25, 0x01,                     //   Logical Maximum(1),
        0x05, 0x07,                     //   Usage Page (Key Codes),
        0x19, 0x00,                     //   Usage Minimum (0),
        0x29, 0x87,                     //   Usage Maximum (), - (REPORT_BYTES-1)*8-1
        0x81, 0x02,                     //   Input (Data, Variable, Absolute),
        0xc0                            // End Collection
    ];

    let mut keyboard = UsbHidClassBuilder::new(&usb_bus)
        .new_interface(NKRO_BOOT_KEYBOARD_REPORT_DESCRIPTOR)
        .description("Keyboard")
        .boot_device(InterfaceProtocol::Keyboard)
        .idle_default(Milliseconds(500))
        .unwrap()
        .in_endpoint(UsbPacketSize::Size32, Milliseconds(20))
        .unwrap()
        .without_out_endpoint()
        .build_interface()
        .unwrap()
        .build()
        .unwrap();

    //https://pid.codes
    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x1209, 0x0001))
        .manufacturer("DLKJ")
        .product("Keyboard")
        .serial_number("TEST")
        .device_class(3) // HID - from: https://www.usb.org/defined-class-codes
        .composite_with_iads()
        .supports_remote_wakeup(false)
        .max_packet_size_0(8)
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
        if button.is_low().unwrap() {
            hal::rom_data::reset_to_usb_boot(0x1 << 13, 0x0);
        }
        if usb_dev.poll(&mut [&mut keyboard]) {
            let keys = [
                if in0.is_low().unwrap() {
                    Keyboard::KeypadNumLockAndClear
                } else {
                    Keyboard::NoEventIndicated
                }, //Numlock
                if in1.is_low().unwrap() {
                    Keyboard::UpArrow
                } else {
                    Keyboard::NoEventIndicated
                }, //Up
                if in2.is_low().unwrap() {
                    Keyboard::F12
                } else {
                    Keyboard::NoEventIndicated
                }, //F12
                if in3.is_low().unwrap() {
                    Keyboard::LeftArrow
                } else {
                    Keyboard::NoEventIndicated
                }, //Left
                if in4.is_low().unwrap() {
                    Keyboard::DownArrow
                } else {
                    Keyboard::NoEventIndicated
                }, //Down
                if in5.is_low().unwrap() {
                    Keyboard::RightArrow
                } else {
                    Keyboard::NoEventIndicated
                }, //Right
                if in6.is_low().unwrap() {
                    Keyboard::A
                } else {
                    Keyboard::NoEventIndicated
                }, //A
                if in7.is_low().unwrap() {
                    Keyboard::B
                } else {
                    Keyboard::NoEventIndicated
                }, //B
                if in8.is_low().unwrap() {
                    Keyboard::C
                } else {
                    Keyboard::NoEventIndicated
                }, //C
                if in9.is_low().unwrap() {
                    Keyboard::LeftControl
                } else {
                    Keyboard::NoEventIndicated
                }, //LCtrl
                if in10.is_low().unwrap() {
                    Keyboard::LeftShift
                } else {
                    Keyboard::NoEventIndicated
                }, //LShift
                if in11.is_low().unwrap() {
                    Keyboard::ReturnEnter
                } else {
                    Keyboard::NoEventIndicated
                }, //Enter
            ];

            let data = &mut [0];
            match keyboard.get_interface_mut(0).unwrap().read_report(data) {
                Err(UsbError::WouldBlock) => {
                    //do nothing
                }
                Err(e) => {
                    panic!("Failed to read keyboard report: {:?}", e)
                }
                Ok(_) => {
                    //send scroll lock to the led
                    led_pin
                        .set_state(PinState::from(KeyboardLeds::unpack(data).unwrap().num_lock))
                        .ok();
                }
            }

            let mut report = [0; 25];
            let mut error = false;
            let mut i = 2;
            let len = 8;

            for k in keys
                .into_iter()
                .filter(|&k| k != Keyboard::NoEventIndicated)
            {
                if (Keyboard::LeftControl..=Keyboard::RightGUI).contains(&k) {
                    report[0] |= 0x1 << (k as u8 - Keyboard::LeftControl as u8)
                } else {
                    let byte = (k as usize) / 8;
                    let bit = (k as u8) % 8;

                    report[8 + byte] |= 1 << bit;

                    if error {
                        continue;
                    }

                    if i < len {
                        report[i] = k as u8;
                        i += 1;
                    } else {
                        error = true;
                        i = len;
                        for i in 2..8 {
                            report[i] = Keyboard::ErrorRollOver as u8;
                        }
                    }
                }
            }

            match keyboard.get_interface_mut(0).unwrap().write_report(&report) {
                Err(UsbError::WouldBlock) => {}
                Ok(_) => {}
                Err(e) => {
                    panic!("Failed to write keyboard report: {:?}", e)
                }
            };
        }
    }
}
