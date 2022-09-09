#![no_std]
#![no_main]

use bsp::entry;
use bsp::hal;
use core::convert::Infallible;
use defmt::*;
use defmt_rtt as _;
use embedded_hal::digital::v2::*;
use embedded_hal::prelude::*;
use embedded_time::duration::Milliseconds;
use embedded_time::rate::Fraction;
use embedded_time::Instant;
use hal::pac;
use hal::Timer;
use panic_probe as _;
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_human_interface_device::page::Keyboard;
use usbd_human_interface_device::prelude::*;

use rp_pico as bsp;

pub struct TimerClock<'a> {
    timer: &'a Timer,
}

impl<'a> TimerClock<'a> {
    pub fn new(timer: &'a Timer) -> Self {
        Self { timer }
    }
}

impl<'a> embedded_time::clock::Clock for TimerClock<'a> {
    type T = u32;
    const SCALING_FACTOR: Fraction = Fraction::new(1, 1_000_000u32);

    fn try_now(&self) -> Result<Instant<Self>, embedded_time::clock::Error> {
        Ok(Instant::new(self.timer.get_counter_low()))
    }
}

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

    let clock = TimerClock::new(&timer);

    info!("Starting");

    //USB
    let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
        pac.USBCTRL_REGS,
        pac.USBCTRL_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));

    let mut keyboard = UsbHidClassBuilder::new()
        .add_interface(
            usbd_human_interface_device::device::keyboard::NKROBootKeyboardInterface::default_config(&clock),
        )
        .build(&usb_bus);

    //https://pid.codes
    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x1209, 0x0001))
        .manufacturer("usbd-human-interface-device")
        .product("NKRO Keyboard")
        .serial_number("TEST")
        .max_packet_size_0(8)
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

    let mut input_count_down = timer.count_down();
    input_count_down.start(Milliseconds(10));

    let mut tick_count_down = timer.count_down();
    tick_count_down.start(Milliseconds(1));

    loop {
        //Poll the keys every 10ms
        if input_count_down.wait().is_ok() {
            let keys = get_keys(keys);

            match keyboard.interface().write_report(&keys) {
                Err(UsbHidError::WouldBlock) => {}
                Err(UsbHidError::Duplicate) => {}
                Ok(_) => {}
                Err(e) => {
                    core::panic!("Failed to write keyboard report: {:?}", e)
                }
            };
        }

        //Tick once per ms
        if tick_count_down.wait().is_ok() {
            match keyboard.interface().tick() {
                Err(UsbHidError::WouldBlock) => {}
                Ok(_) => {}
                Err(e) => {
                    core::panic!("Failed to process keyboard tick: {:?}", e)
                }
            };
        }

        if usb_dev.poll(&mut [&mut keyboard]) {
            match keyboard.interface().read_report() {
                Err(UsbError::WouldBlock) => {
                    //do nothing
                }
                Err(e) => {
                    core::panic!("Failed to read keyboard report: {:?}", e)
                }
                Ok(leds) => {
                    led_pin.set_state(PinState::from(leds.num_lock)).ok();
                }
            }
        }
    }
}

fn get_keys(keys: &[&dyn InputPin<Error = Infallible>]) -> [Keyboard; 12] {
    [
        if keys[0].is_low().unwrap() {
            Keyboard::KeypadNumLockAndClear
        } else {
            Keyboard::NoEventIndicated
        }, //Numlock
        if keys[1].is_low().unwrap() {
            Keyboard::UpArrow
        } else {
            Keyboard::NoEventIndicated
        }, //Up
        if keys[2].is_low().unwrap() {
            Keyboard::F12
        } else {
            Keyboard::NoEventIndicated
        }, //F12
        if keys[3].is_low().unwrap() {
            Keyboard::LeftArrow
        } else {
            Keyboard::NoEventIndicated
        }, //Left
        if keys[4].is_low().unwrap() {
            Keyboard::DownArrow
        } else {
            Keyboard::NoEventIndicated
        }, //Down
        if keys[5].is_low().unwrap() {
            Keyboard::RightArrow
        } else {
            Keyboard::NoEventIndicated
        }, //Right
        if keys[6].is_low().unwrap() {
            Keyboard::A
        } else {
            Keyboard::NoEventIndicated
        }, //A
        if keys[7].is_low().unwrap() {
            Keyboard::B
        } else {
            Keyboard::NoEventIndicated
        }, //B
        if keys[8].is_low().unwrap() {
            Keyboard::C
        } else {
            Keyboard::NoEventIndicated
        }, //C
        if keys[9].is_low().unwrap() {
            Keyboard::LeftControl
        } else {
            Keyboard::NoEventIndicated
        }, //LCtrl
        if keys[10].is_low().unwrap() {
            Keyboard::LeftShift
        } else {
            Keyboard::NoEventIndicated
        }, //LShift
        if keys[11].is_low().unwrap() {
            Keyboard::ReturnEnter
        } else {
            Keyboard::NoEventIndicated
        }, //Enter
    ]
}
