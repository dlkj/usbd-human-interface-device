//!Implements HID mouse devices
pub mod descriptors;

use crate::hid::HIDDevice;
use embedded_time::duration::*;
use log::*;
use usb_device::class_prelude::*;
use usb_device::Result;

/// HIDMouse provides an interface to send mouse movement and button presses to
/// the host device
pub trait HIDMouse {
    /// Writes an input report given representing the update to the mouse state
    /// to the host system
    fn write_mouse(&self, buttons: u8, x: i8, y: i8) -> Result<()>;
}

/// Implements a HID Mouse that conforms to the Boot specification. This aims
/// to be compatible with BIOS and other reduced functionality USB hosts
///
/// This is defined in Appendix B.1 of Device Class Definition for Human
/// Interface Devices (HID) Version 1.11 -
/// https://www.usb.org/sites/default/files/hid1_11.pdf
pub struct HIDBootMouse<'a, B: UsbBus> {
    hid_device: &'a HIDDevice<'a, B>,
}

impl<'a, B: UsbBus> HIDBootMouse<'a, B> {
    pub fn new<I: Into<Milliseconds>>(
        hid: &'a HIDDevice<'a, B>,
        interval: I,
    ) -> HIDBootMouse<'a, B> {
        HIDBootMouse { hid_device: hid }
    }
}
