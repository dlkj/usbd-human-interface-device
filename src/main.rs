//! # HID USB Example for usbd-hid-devices

#![no_std]
#![no_main]

use crate::hal::Timer;
use cortex_m_rt::entry;
use embedded_hal::digital::v2::ToggleableOutputPin;
use embedded_hal::prelude::*;
use embedded_time::duration::*;
use nb::block;
use panic_halt as _;
use rp_pico::hal;
use rp_pico::hal::pac;

#[entry]
fn main() -> ! {
    let mut pac = pac::Peripherals::take().unwrap();

    let mut watchdog = hal::Watchdog::new(pac.WATCHDOG);
    let _clocks = hal::clocks::init_clocks_and_plls(
        rp_pico::XOSC_CRYSTAL_FREQ,
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
    let pins = rp_pico::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    let timer = Timer::new(pac.TIMER, &mut pac.RESETS);
    let mut led_pin = pins.gpio13.into_push_pull_output();

    let mut count_down = timer.count_down();
    count_down.start(500.milliseconds());

    loop {
        led_pin.toggle().unwrap();
        block!(count_down.wait()).unwrap();
    }
}
