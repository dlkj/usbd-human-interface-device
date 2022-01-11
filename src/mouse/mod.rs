//!Implements HID mouse devices
pub mod descriptors;

use crate::hid::InterfaceProtocol;
use crate::hid::InterfaceSubClass;
use log::warn;
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
#[derive(Default)]
pub struct HIDBootMouse {}

impl super::hid::HIDClass for HIDBootMouse {
    fn packet_size(&self) -> u8 {
        8
    }
    fn poll_interval(&self) -> embedded_time::duration::Milliseconds {
        embedded_time::duration::Milliseconds(10)
    }
    fn report_descriptor(&self) -> &'static [u8] {
        &descriptors::HID_BOOT_MOUSE_REPORT_DESCRIPTOR
    }
    fn interface_sub_class(&self) -> InterfaceSubClass {
        InterfaceSubClass::Boot
    }
    fn interface_protocol(&self) -> InterfaceProtocol {
        InterfaceProtocol::Mouse
    }
    fn reset(&mut self) {}
    fn interface_name(&self) -> &str {
        "Mouse"
    }
}

impl<B: UsbBus> HIDMouse for super::hid::HID<'_, B, HIDBootMouse> {
    fn write_mouse(
        &self,
        buttons: u8,
        x: i8,
        y: i8,
    ) -> core::result::Result<(), usb_device::UsbError> {
        let data = [buttons, x.to_le_bytes()[0], y.to_le_bytes()[0]];

        match self.write_report(&data) {
            Ok(3) => Ok(()),
            Ok(n) => {
                warn!("Sent {:X} bytes, expected 3 byte", n,);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}
