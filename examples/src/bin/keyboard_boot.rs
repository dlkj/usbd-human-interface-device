#![no_std]
#![no_main]

use core::convert::Infallible;

use adafruit_macropad::hal;
use cortex_m_rt::entry;
use embedded_hal::digital::v2::*;
use embedded_hal::prelude::*;
use embedded_time::duration::Milliseconds;
use embedded_time::rate::Hertz;
use hal::pac;
use hal::timer::CountDown;
use hal::Clock;
use log::*;
use packed_struct::prelude::*;
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_hid_devices::device::keyboard::{new_boot_keyboard, BootKeyboardReport, KeyboardLeds};
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

    init_logger(oled_spi, oled_dc.into(), oled_cs.into(), &button);
    info!("Starting up...");

    //USB
    let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
        pac.USBCTRL_REGS,
        pac.USBCTRL_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));

    let mut keyboard = new_boot_keyboard().build(&usb_bus);

    //https://pid.codes
    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x1209, 0x0001))
        .manufacturer("usbd-hid-devices")
        .product("Boot Keyboard")
        .serial_number("TEST")
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

    let mut last_keys = None;

    let mut input_count_down = timer.count_down();
    input_count_down.start(Milliseconds(10));

    let mut idle_count_down = reset_idle(&timer, keyboard.interface().global_idle());

    let mut display_poll = timer.count_down();
    display_poll.start(DISPLAY_POLL);

    loop {
        if button.is_low().unwrap() {
            hal::rom_data::reset_to_usb_boot(0x1 << 13, 0x0);
        }

        //Poll the keys every 10ms
        if input_count_down.wait().is_ok() {
            if idle_count_down
                .as_mut()
                .map(|c| c.wait().is_ok())
                .unwrap_or(false)
            {
                //Expire on idle
                last_keys = None;
            }

            let keys = get_keys(keys);

            if last_keys.map(|k| k != keys).unwrap_or(true) {
                match keyboard
                    .interface()
                    .write_report(&BootKeyboardReport::new(keys).pack().unwrap())
                {
                    Err(UsbError::WouldBlock) => {}
                    Ok(_) => {
                        last_keys = Some(keys);
                        idle_count_down = reset_idle(&timer, keyboard.interface().global_idle());
                    }
                    Err(e) => {
                        panic!("Failed to write keyboard report: {:?}", e)
                    }
                };
            }
        }

        if usb_dev.poll(&mut [&mut keyboard]) {
            let data = &mut [0];
            match keyboard.interface().read_report(data) {
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
        }

        if display_poll.wait().is_ok() {
            log::logger().flush();
        }
    }
}

fn reset_idle(timer: &hal::Timer, idle: Milliseconds) -> Option<CountDown> {
    if idle <= Milliseconds(0_u32) {
        None
    } else {
        let mut count_down = timer.count_down();
        count_down.start(idle);
        Some(count_down)
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
