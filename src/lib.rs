//! USB Hid devices
#![no_std]
extern crate num;
#[macro_use]
extern crate num_derive;

//Allow the use of std in tests
#[cfg(test)]
#[macro_use]
extern crate std;

pub mod hid;
pub mod keyboard;
pub mod mouse;
