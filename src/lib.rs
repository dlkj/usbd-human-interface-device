//! Batteries included embedded USB HID library for [usb-device](https://crates.io/crates/usb-device). Includes concrete
//! Keyboard (boot and NKRO), Mouse and Consumer Control implementations as well as support for building your own HID classes.
//!
//! This library has been tested on the RP2040 but should work on any platform supported by [usb-device](https://crates.io/crates/usb-device).
//!
//! Devices created with this library should work with any USB host. It has been tested on Windows, Linux and Android. MacOS
//! should work but has not been verified.
//!
//! ```rust,no_run
//! use usbd_human_interface_device::page::Keyboard;
//! use usbd_human_interface_device::device::keyboard::NKROBootKeyboardInterface;
//! use usbd_human_interface_device::prelude::*;
//!
//! let usb_alloc = UsbBusAllocator::new(usb_bus);
//!
//! let mut keyboard = UsbHidClassBuilder::new()
//!     .add_interface(
//!         NKROBootKeyboardInterface::default_config(&clock),
//!     )
//!     .build(&usb_alloc);
//!
//! let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
//!     .manufacturer("usbd-human-interface-device")
//!     .product("NKRO Keyboard")
//!     .serial_number("TEST")
//!     .build();
//!
//! loop {
//!
//!     let keys = if pin.is_high().unwrap() {
//!             &[Keyboard::A]
//!         } else {
//!             &[]
//!     };
//!     
//!     keyboard.interface().write_report(keys).ok();
//!     keyboard.interface().tick().unwrap();
//!     
//!     if usb_dev.poll(&mut [&mut keyboard]) {
//!         match keyboard.interface().read_report() {
//!
//!             Ok(l) => {
//!                 update_leds(l);
//!             }
//!             _ => {}
//!         }
//!     }
//! }
//! ```
//!
//! Features
//! --------
//!
//! * Keyboard implementations - standard boot compliant keyboard, boot compatible NKRO(N-Key Roll Over) keyboard
//! * Mouse - standard boot compliant mouse, boot compatible mouse with scroll wheel and pan
//! * Consumer Control - fixed function media control device, arbitrary consumer control device
//! * Enums defining the Consumer, Desktop, Game, Keyboard, LED, Simulation and Telephony HID usage pages
//! * Support for multi-interface devices
//! * Support for HID idle
//! * Support for HID protocol changing
//! * Support for both single and multiple reports
//!
//! Examples
//! --------
//!
//! See [examples](https://github.com/dlkj/usbd-human-interface-device/tree/main/examples/src/bin) for demonstration of how
//! to use this library on the RP2040 (Raspberry Pi Pico)!

#![no_std]

//Allow the use of std in tests
#[cfg(test)]
#[macro_use]
extern crate std;

use usb_device::UsbError;

pub mod device;
pub mod hid_class;
pub mod interface;
pub mod page;
pub mod prelude;

#[derive(Debug)]
pub enum UsbHidError {
    WouldBlock,
    Duplicate,
    UsbError(UsbError),
    SerializationError,
}

impl From<UsbError> for UsbHidError {
    fn from(e: UsbError) -> Self {
        match e {
            UsbError::WouldBlock => UsbHidError::WouldBlock,
            _ => UsbHidError::UsbError(e),
        }
    }
}
