#![no_std]
#![no_main]

use bsp::entry;
use bsp::hal;
use defmt::*;
use defmt_rtt as _;
use embedded_hal::digital::v2::*;
use embedded_hal::prelude::*;
use fugit::ExtU32;
use hal::pac;
use panic_probe as _;
#[allow(clippy::wildcard_imports)]
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_human_interface_device::device::mouse::BootMouseReport;
use usbd_human_interface_device::prelude::*;

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

    let mut mouse = UsbHidClassBuilder::new()
        .add(usbd_human_interface_device::device::mouse::BootMouseConfig::default())
        .build(&usb_bus);

    //https://pid.codes
    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x1209, 0x0001))
        .manufacturer("usbd-human-interface-device")
        .product("Boot Mouse")
        .serial_number("TEST")
        .build();

    //GPIO pins
    let mut led_pin = pins.gpio13.into_push_pull_output();

    let input_pins: [&dyn InputPin<Error = core::convert::Infallible>; 7] = [
        &pins.gpio1.into_pull_up_input(),
        &pins.gpio2.into_pull_up_input(),
        &pins.gpio3.into_pull_up_input(),
        &pins.gpio4.into_pull_up_input(),
        &pins.gpio5.into_pull_up_input(),
        &pins.gpio6.into_pull_up_input(),
        &pins.gpio7.into_pull_up_input(),
    ];

    led_pin.set_low().ok();

    let mut last_buttons = 0;
    let mut report = BootMouseReport::default();

    let mut input_count_down = timer.count_down();
    input_count_down.start(10.millis());

    loop {
        //Poll every 10ms
        if input_count_down.wait().is_ok() {
            report = update_report(report, &input_pins);

            //Only write a report if the mouse is moving or buttons change
            if report.buttons != last_buttons || report.x != 0 || report.y != 0 {
                match mouse.interface().write_report(&report) {
                    Err(UsbHidError::WouldBlock) => {}
                    Ok(_) => {
                        last_buttons = report.buttons;
                        report = BootMouseReport::default();
                    }
                    Err(e) => {
                        core::panic!("Failed to write mouse report: {:?}", e)
                    }
                }
            }
        }

        if usb_dev.poll(&mut [&mut mouse]) {}
    }
}

fn update_report(
    mut report: BootMouseReport,
    pins: &[&dyn InputPin<Error = core::convert::Infallible>; 7],
) -> BootMouseReport {
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
