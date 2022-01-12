//! USB Hid devices
#![no_std]

//Allow the use of std in tests
#[cfg(test)]
#[macro_use]
extern crate std;

pub mod hid;
pub mod keyboard;
pub mod mouse;
