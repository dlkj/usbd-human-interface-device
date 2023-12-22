#![no_std]
#![no_main]

use bsp::entry;
use bsp::hal;
use defmt::*;
use defmt_rtt as _;
use fugit::ExtU32;
use hal::pac;
use panic_probe as _;
#[allow(clippy::wildcard_imports)]
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_human_interface_device::interface::{InBytes64, OutBytes64, ReportSingle};
use usbd_human_interface_device::usb_class::prelude::*;

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

    info!("Starting");

    //USB
    let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
        pac.USBCTRL_REGS,
        pac.USBCTRL_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));

    #[rustfmt::skip]
    pub const HID_IO_REPORT_DESCRIPTOR: &[u8] = &[
        0x06, 0x1C, 0xFF, // Usage Page (Vendor Defined 0xFF1C),
        0x0A, 0x01, 0x11, // Usage (0x1100)
        0xA1, 0x01, // Collection (Application),
        0x09, 0x01, //   Usage (0x01),
        0x15, 0x00, //       Logical Minimum(0),
        0x26, 0xFF, 0x00, // Logical Max (0x00FF),
        0x75, 0x08, //       Report size (8)
        0x95, 0x40, //       Report count (64)
        0x81, 0x02, //       Input (Data | Variable | Absolute)
        0x09, 0x02, //   Usage (0x02),
        0x15, 0x00, //       Logical Minimum(0),
        0x26, 0xFF, 0x00, // Logical Max (0x00FF),
        0x75, 0x08, //       Report size (8)
        0x95, 0x40, //       Report count (64)
        0x91, 0x02, //       Output (Data | Variable | Absolute)
        0xC0,       // End Collection
    ];

    let mut hid_io = UsbHidClassBuilder::new()
        .add_device(
            InterfaceBuilder::<InBytes64, OutBytes64, ReportSingle>::new(HID_IO_REPORT_DESCRIPTOR)
                .unwrap()
                .description("Custom HID IO Device")
                .in_endpoint(5.millis())
                .unwrap()
                .with_out_endpoint(5.millis())
                .unwrap()
                .build(),
        )
        .build(&usb_bus);

    //https://pid.codes
    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x1209, 0x0008))
        .manufacturer("usbd-human-interface-device")
        .product("hid io")
        .serial_number("TEST")
        .build();

    loop {
        if usb_dev.poll(&mut [&mut hid_io]) {
            let data = &mut [0u8; 64];
            match hid_io.device().read_report(data) {
                Err(UsbError::WouldBlock) => {
                    //do nothing
                }
                Err(e) => {
                    core::panic!("Failed to read report: {:?}", e)
                }
                Ok(_) => {
                    info!("received: {:?}", data);
                }
            }
        }
    }
}
