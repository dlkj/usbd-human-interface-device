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
use usbd_human_interface_device::device::consumer::MultipleConsumerReport;
use usbd_human_interface_device::prelude::*;

use usbd_human_interface_device::page::Consumer;

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
        .add_device(usbd_human_interface_device::device::consumer::ConsumerControlConfig::default())
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

    let mut input_pins: [Pin<DynPinId, FunctionSioInput, PullUp>; 9] = [
        pins.gpio1.into_pull_up_input().into_dyn_pin(),
        pins.gpio2.into_pull_up_input().into_dyn_pin(),
        pins.gpio3.into_pull_up_input().into_dyn_pin(),
        pins.gpio4.into_pull_up_input().into_dyn_pin(),
        pins.gpio5.into_pull_up_input().into_dyn_pin(),
        pins.gpio6.into_pull_up_input().into_dyn_pin(),
        pins.gpio7.into_pull_up_input().into_dyn_pin(),
        pins.gpio8.into_pull_up_input().into_dyn_pin(),
        pins.gpio9.into_pull_up_input().into_dyn_pin(),
    ];

    led_pin.set_low().ok();

    let mut last = get_report(&mut input_pins);

    let mut input_count_down = timer.count_down();
    input_count_down.start(50.millis());

    loop {
        //Poll the every 10ms
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

        if usb_dev.poll(&mut [&mut consumer]) {}
    }
}

fn get_report(pins: &mut [Pin<DynPinId, FunctionSioInput, PullUp>; 9]) -> MultipleConsumerReport {
    #[rustfmt::skip]
        let pins = [
        if pins[0].is_low().unwrap() { Consumer::PlayPause } else { Consumer::Unassigned },
        if pins[1].is_low().unwrap() { Consumer::ScanPreviousTrack } else { Consumer::Unassigned },
        if pins[2].is_low().unwrap() { Consumer::ScanNextTrack } else { Consumer::Unassigned },
        if pins[3].is_low().unwrap() { Consumer::Mute } else { Consumer::Unassigned },
        if pins[4].is_low().unwrap() { Consumer::VolumeDecrement } else { Consumer::Unassigned },
        if pins[5].is_low().unwrap() { Consumer::VolumeIncrement } else { Consumer::Unassigned },
        if pins[6].is_low().unwrap() { Consumer::ALCalculator } else { Consumer::Unassigned },
        if pins[7].is_low().unwrap() { Consumer::ALInternetBrowser } else { Consumer::Unassigned },
        if pins[8].is_low().unwrap() { Consumer::ALFileBrowser } else { Consumer::Unassigned },
    ];

    let mut report = MultipleConsumerReport {
        codes: [Consumer::Unassigned; 4],
    };

    let mut it = pins.iter().filter(|&&c| c != Consumer::Unassigned);
    for c in report.codes.iter_mut() {
        if let Some(&code) = it.next() {
            *c = code;
        } else {
            break;
        }
    }
    report
}
