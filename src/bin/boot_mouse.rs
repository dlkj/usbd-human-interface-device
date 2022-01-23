#![no_std]
#![no_main]

use adafruit_macropad::hal;
use cortex_m_rt::entry;
use embedded_hal::digital::v2::*;
use embedded_hal::prelude::_embedded_hal_timer_CountDown;
use embedded_time::duration::Milliseconds;
use embedded_time::rate::Hertz;
use hal::pac;
use hal::Clock;
use log::*;
use nb::block;
use packed_struct::prelude::*;
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_hid_devices::device::mouse::BootMouseReport;
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

    let timer = hal::timer::Timer::new(pac.TIMER, &mut pac.RESETS);

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

    let mut mouse = usbd_hid_devices::device::mouse::new_boot_mouse(&usb_bus)
        .build()
        .unwrap();

    //https://pid.codes
    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x1209, 0x0001))
        .manufacturer("DLKJ")
        .product("Mouse")
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

    let mut last = BootMouseReport {
        buttons: 0,
        x: 0,
        y: 0,
    };

    loop {
        if button.is_low().unwrap() {
            hal::rom_data::reset_to_usb_boot(0x1 << 13, 0x0);
        }

        let report = read_keys(keys);

        //Only write a report if the mouse is moving or buttons change
        if report != last || report.x != 0 || report.y != 0 {
            match mouse
                .get_interface_mut(0)
                .unwrap()
                .write_report(&report.pack().unwrap())
            {
                Err(UsbError::WouldBlock) => {}
                Ok(_) => {
                    last = report;
                }
                Err(e) => {
                    panic!("Failed to write mouse report: {:?}", e)
                }
            }
        }

        if usb_dev.poll(&mut [&mut mouse]) {}

        //spin for 5 ms
        let mut cd = timer.count_down();
        cd.start(Milliseconds(5));
        block!(cd.wait()).unwrap();
    }
}

fn read_keys(keys: &[&dyn InputPin<Error = core::convert::Infallible>]) -> BootMouseReport {
    let mut buttons = 0;
    let mut x = 0;
    let mut y = 0;

    if keys[0].is_low().unwrap() {
        buttons |= 0x1; //Left
    }
    if keys[1].is_low().unwrap() {
        buttons |= 0x4; //Middle
    }
    if keys[2].is_low().unwrap() {
        buttons |= 0x2; //Right
    }
    if keys[3].is_low().unwrap() {}
    if keys[4].is_low().unwrap() {
        y += -10; //Up
    }
    if keys[5].is_low().unwrap() {}
    if keys[6].is_low().unwrap() {
        x += -10; //Left
    }
    if keys[7].is_low().unwrap() {
        y += 10; //Down
    }
    if keys[8].is_low().unwrap() {
        x += 10; //Right
    }
    if keys[9].is_low().unwrap() {}
    if keys[10].is_low().unwrap() {}
    if keys[11].is_low().unwrap() {}

    BootMouseReport { buttons, x, y }
}
