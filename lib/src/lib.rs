//! Batteries included USB Human Interface Device (HID) library
//!
//! Based on [`usb-device`](<https://crates.io/crates/usb-device>)
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
