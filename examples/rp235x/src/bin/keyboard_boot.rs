#![no_std]
#![no_main]

use panic_halt as _;

use embedded_hal::digital::*;
use embedded_hal_0_2::timer::CountDown;
use fugit::ExtU32;
use rp235x_hal::{
    self as hal,
    gpio::{DynPinId, FunctionSioInput, Pin, PullUp},
};
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_human_interface_device::page::Keyboard;
use usbd_human_interface_device::prelude::UsbHidClassBuilder;
use usbd_human_interface_device::prelude::*;

#[link_section = ".start_block"]
#[used]
pub static IMAGE_DEF: hal::block::ImageDef = hal::block::ImageDef::secure_exe();

const XTAL_FREQ_HZ: u32 = 12_000_000u32;

#[hal::entry]
fn main() -> ! {
    let mut pac = hal::pac::Peripherals::take().unwrap();

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
    .unwrap();

    let timer = hal::Timer::new_timer0(pac.TIMER0, &mut pac.RESETS, &clocks);

    let sio = hal::Sio::new(pac.SIO);
    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
        pac.USB,
        pac.USB_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));

    let mut keyboard = UsbHidClassBuilder::new()
        .add_device(usbd_human_interface_device::device::keyboard::BootKeyboardConfig::default())
        .build(&usb_bus);

    //https://pid.codes
    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x1209, 0x0001))
        .strings(&[StringDescriptors::default()
            .manufacturer("usbd-human-interface-device")
            .product("Boot Keyboard")
            .serial_number("TEST")])
        .unwrap()
        .build();

    //GPIO pins
    let mut led_pin = pins.gpio13.into_push_pull_output();

    let mut keys: [Pin<DynPinId, FunctionSioInput, PullUp>; 12] = [
        pins.gpio1.into_pull_up_input().into_dyn_pin(),
        pins.gpio2.into_pull_up_input().into_dyn_pin(),
        pins.gpio3.into_pull_up_input().into_dyn_pin(),
        pins.gpio4.into_pull_up_input().into_dyn_pin(),
        pins.gpio5.into_pull_up_input().into_dyn_pin(),
        pins.gpio6.into_pull_up_input().into_dyn_pin(),
        pins.gpio7.into_pull_up_input().into_dyn_pin(),
        pins.gpio8.into_pull_up_input().into_dyn_pin(),
        pins.gpio9.into_pull_up_input().into_dyn_pin(),
        pins.gpio10.into_pull_up_input().into_dyn_pin(),
        pins.gpio11.into_pull_up_input().into_dyn_pin(),
        pins.gpio12.into_pull_up_input().into_dyn_pin(),
    ];

    let mut input_count_down = timer.count_down();
    input_count_down.start(10.millis());

    let mut tick_count_down = timer.count_down();
    tick_count_down.start(1.millis());

    loop {
        //Poll the keys every 10ms
        if input_count_down.wait().is_ok() {
            let keys = get_keys(&mut keys);

            match keyboard.device().write_report(keys) {
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
            match keyboard.tick() {
                Err(UsbHidError::WouldBlock) => {}
                Ok(_) => {}
                Err(e) => {
                    core::panic!("Failed to process keyboard tick: {:?}", e)
                }
            };
        }

        if usb_dev.poll(&mut [&mut keyboard]) {
            match keyboard.device().read_report() {
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

fn get_keys(keys: &mut [Pin<DynPinId, FunctionSioInput, PullUp>; 12]) -> [Keyboard; 12] {
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

#[link_section = ".bi_entries"]
#[used]
pub static PICOTOOL_ENTRIES: [hal::binary_info::EntryAddr; 3] = [
    // hal::binary_info::rp_cargo_bin_name!(),
    hal::binary_info::rp_cargo_version!(),
    hal::binary_info::rp_program_description!(c"USB HID Example"),
    // hal::binary_info::rp_cargo_homepage_url!(),
    hal::binary_info::rp_program_build_attribute!(),
];
