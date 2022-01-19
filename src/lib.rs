//! Batteries included USB Human Interface Device (HID) library
//!
//! Based on [`usb-device`](<https://crates.io/crates/usb-device>)
#![no_std]

//Allow the use of std in tests
#[cfg(test)]
#[macro_use]
extern crate std;

pub mod device;
pub mod hid_class;
pub mod page;
