#![no_std]
#![no_main]

use bsp::entry;
use bsp::hal;
use cortex_m::prelude::*;
use defmt::*;
use defmt_rtt as _;
use embedded_hal::digital::*;
use fugit::ExtU32;
use hal::{
    gpio::{DynPinId, FunctionSioInput, Pin, PullUp},
    pac,
};
use panic_probe as _;

#[allow(clippy::wildcard_imports)]
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_human_interface_device::device::consumer::FixedFunctionReport;
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

    let timer = hal::Timer::new(pac.TIMER, &mut pac.RESETS, &clocks);

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

    let mut consumer = UsbHidClassBuilder::new()
        .add_device(
            usbd_human_interface_device::device::consumer::ConsumerControlFixedConfig::default(),
        )
        .build(&usb_bus);
    //https://pid.codes
    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x1209, 0x0001))
        .strings(&[StringDescriptors::default()
            .manufacturer("usbd-human-interface-device")
            .product("Consumer Control")
            .serial_number("TEST")])
        .unwrap()
        .build();

    //GPIO pins
    let mut led_pin = pins.gpio13.into_push_pull_output();

    let mut input_pins: [Pin<DynPinId, FunctionSioInput, PullUp>; 7] = [
        pins.gpio1.into_pull_up_input().into_dyn_pin(),
        pins.gpio2.into_pull_up_input().into_dyn_pin(),
        pins.gpio3.into_pull_up_input().into_dyn_pin(),
        pins.gpio4.into_pull_up_input().into_dyn_pin(),
        pins.gpio5.into_pull_up_input().into_dyn_pin(),
        pins.gpio6.into_pull_up_input().into_dyn_pin(),
        pins.gpio7.into_pull_up_input().into_dyn_pin(),
    ];

    led_pin.set_low().ok();

    let mut input_count_down = timer.count_down();
    input_count_down.start(50.millis());

    let mut last = get_report(&mut input_pins);

    loop {
        //Poll every 10ms
        if input_count_down.wait().is_ok() {
            let report = get_report(&mut input_pins);
            if report != last {
                match consumer.device().write_report(&report) {
                    Err(UsbError::WouldBlock) => {}
                    Ok(_) => {
                        last = report;
                    }
                    Err(e) => {
                        core::panic!("Failed to write consumer report: {:?}", e)
                    }
                }
            }
        }

        usb_dev.poll(&mut [&mut consumer]);
    }
}

fn get_report(pins: &mut [Pin<DynPinId, FunctionSioInput, PullUp>; 7]) -> FixedFunctionReport {
    FixedFunctionReport {
        next: pins[0].is_low().unwrap(),
        previous: pins[1].is_low().unwrap(),
        stop: pins[2].is_low().unwrap(),
        play_pause: pins[3].is_low().unwrap(),
        mute: pins[4].is_low().unwrap(),
        volume_increment: pins[5].is_low().unwrap(),
        volume_decrement: pins[6].is_low().unwrap(),
    }
}
