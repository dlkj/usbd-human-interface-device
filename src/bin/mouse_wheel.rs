#![no_std]
#![no_main]

use adafruit_macropad::hal;
use cortex_m_rt::entry;
use embedded_hal::digital::v2::*;
use embedded_hal::prelude::*;
use embedded_time::duration::Milliseconds;
use embedded_time::rate::Hertz;
use hal::pac;
use hal::Clock;
use log::*;
use packed_struct::prelude::*;
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_hid_devices::device::mouse::{WheelMouseReport, WHEEL_MOUSE_REPORT_DESCRIPTOR};
use usbd_hid_devices::hid_class::prelude::*;
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
    let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
        pac.USBCTRL_REGS,
        pac.USBCTRL_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));

    let mut mouse = UsbHidClassBuilder::new(&usb_bus)
        .new_interface(WHEEL_MOUSE_REPORT_DESCRIPTOR)
        .boot_device(InterfaceProtocol::Mouse)
        .description("Wheel Mouse")
        .idle_default(Milliseconds(0))
        .unwrap()
        .in_endpoint(UsbPacketSize::Size8, Milliseconds(10))
        .unwrap()
        .without_out_endpoint()
        .build_interface()
        .unwrap()
        .build()
        .unwrap();

    //https://pid.codes
    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x1209, 0x0001))
        .manufacturer("usbd-hid-devices")
        .product("Wheel Mouse")
        .serial_number("TEST")
        .device_class(3) // HID - from: https://www.usb.org/defined-class-codes
        .composite_with_iads()
        .supports_remote_wakeup(false)
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

    let mut last_buttons = 0;
    let mut report = WheelMouseReport {
        buttons: 0,
        x: 0,
        y: 0,
        vertical_wheel: 0,
        horizontal_wheel: 0,
    };

    let mut input_count_down = timer.count_down();
    input_count_down.start(Milliseconds(10));

    loop {
        if button.is_low().unwrap() {
            hal::rom_data::reset_to_usb_boot(0x1 << 13, 0x0);
        }

        //Poll the keys every 10ms
        if input_count_down.wait().is_ok() {
            report = update_report(report, keys);

            //Only write a report if the mouse is moving or buttons change
            if report.buttons != last_buttons
                || report.x != 0
                || report.y != 0
                || report.vertical_wheel != 0
                || report.horizontal_wheel != 0
            {
                match mouse
                    .get_interface_mut(0)
                    .unwrap()
                    .write_report(&report.pack().unwrap())
                {
                    Err(UsbError::WouldBlock) => {}
                    Ok(_) => {
                        last_buttons = report.buttons;
                        report = WheelMouseReport {
                            buttons: 0,
                            x: 0,
                            y: 0,
                            vertical_wheel: 0,
                            horizontal_wheel: 0,
                        }
                    }
                    Err(e) => {
                        panic!("Failed to write mouse report: {:?}", e)
                    }
                }
            }
        }

        if usb_dev.poll(&mut [&mut mouse]) {}
    }
}

fn update_report(
    mut report: WheelMouseReport,
    keys: &[&dyn InputPin<Error = core::convert::Infallible>],
) -> WheelMouseReport {
    if keys[0].is_low().unwrap() {
        report.buttons |= 0x1; //Left
    }
    if keys[1].is_low().unwrap() {
        report.buttons |= 0x4; //Middle
    }
    if keys[2].is_low().unwrap() {
        report.buttons |= 0x2; //Right
    }
    if keys[3].is_low().unwrap() {
        report.vertical_wheel = i8::saturating_add(report.vertical_wheel, -1);
    }
    if keys[4].is_low().unwrap() {
        report.y = i8::saturating_add(report.y, -10); //Up
    }
    if keys[5].is_low().unwrap() {
        report.vertical_wheel = i8::saturating_add(report.vertical_wheel, 1);
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
    if keys[9].is_low().unwrap() {
        report.horizontal_wheel = i8::saturating_add(report.horizontal_wheel, -1);
    }
    if keys[10].is_low().unwrap() {}
    if keys[11].is_low().unwrap() {
        report.horizontal_wheel = i8::saturating_add(report.horizontal_wheel, 1);
    }

    report
}
