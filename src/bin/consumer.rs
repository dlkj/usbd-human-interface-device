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
use usb_device::class_prelude::*;
use usb_device::prelude::*;
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

    #[rustfmt::skip]
    pub const FIXED_BUTTONS_CONSUMER_CONTROL_REPORT_DESCRIPTOR: &[u8] = &[
        0x05, 0x0C, //        Usage Page (Consumer Devices)  
        0x09, 0x01, //        Usage (Consumer Control)  
        0xA1, 0x01, //        Collection (Application)  
        0x05, 0x0C, //            Usage Page (Consumer Devices)  
        0x15, 0x00, //            Logical Minimum (0)  
        0x25, 0x01, //            Logical Maximum (1)  
        0x75, 0x01, //            Report Size (1)  
        0x95, 0x07, //            Report Count (7)  
        0x09, 0xB5, //            Usage (Scan Next Track)  
        0x09, 0xB6, //            Usage (Scan Previous Track)  
        0x09, 0xB7, //            Usage (Stop)  
        0x09, 0xCD, //            Usage (Play/Pause)  
        0x09, 0xE2, //            Usage (Mute)  
        0x09, 0xE9, //            Usage (Volume Increment)  
        0x09, 0xEA, //            Usage (Volume Decrement)  
        0x81, 0x02, //            Input (Data,Var,Abs,NWrp,Lin,Pref,NNul,Bit)  
        0x95, 0x01, //            Report Count (1)  
        0x81, 0x01, //            Input (Cnst,Ary,Abs)  
        0xC0, //        End Collection
    ];

    let mut control =
        UsbHidClassBuilder::new(&usb_bus, FIXED_BUTTONS_CONSUMER_CONTROL_REPORT_DESCRIPTOR)
            .interface_description("Consumer Control")
            .idle_default(Milliseconds(0))
            .unwrap()
            .in_endpoint(UsbPacketSize::Size8, Milliseconds(10))
            .unwrap()
            .without_out_endpoint()
            .build();

    //https://pid.codes
    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x1209, 0x0001))
        .manufacturer("DLKJ")
        .product("Consumer Control")
        .serial_number("TEST")
        .device_class(3) // HID - from: https://www.usb.org/defined-class-codes
        .composite_with_iads()
        .supports_remote_wakeup(true)
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

    led_pin.set_low().ok();

    let mut last = 0xFF;

    loop {
        if button.is_low().unwrap() {
            hal::rom_data::reset_to_usb_boot(0x1 << 13, 0x0);
        }
        if usb_dev.poll(&mut [&mut control]) {
            let keys = [
                if in0.is_low().unwrap() { 0x1 } else { 0x00 },  //next
                if in1.is_low().unwrap() { 0x2 } else { 0x00 },  //previous
                if in2.is_low().unwrap() { 0x4 } else { 0x00 },  //stop
                if in3.is_low().unwrap() { 0x8 } else { 0x00 },  //play pause
                if in4.is_low().unwrap() { 0x10 } else { 0x00 }, //mute
                if in5.is_low().unwrap() { 0x20 } else { 0x00 }, //vol+
                if in6.is_low().unwrap() { 0x40 } else { 0x00 }, //vol-
            ];

            let mut buff = [0; 64];
            match control.read_report(&mut buff) {
                Err(UsbError::WouldBlock) => {
                    //do nothing
                }
                Err(e) => {
                    panic!("Failed to read consumer report: {:?}", e)
                }
                Ok(_) => {}
            }

            let mut report_code = keys.into_iter().reduce(|a, b| (a | b)).unwrap_or(0);

            if report_code != last {
                last = report_code;
                report_code = 0;
            }

            match control.write_report(&[report_code]) {
                Err(UsbError::WouldBlock) => {}
                Ok(_) => {}
                Err(e) => {
                    panic!("Failed to write consumer report: {:?}", e)
                }
            };
        }
    }
}
