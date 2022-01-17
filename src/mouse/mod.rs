//!Implements Hid mouse devices
pub mod descriptors;

use super::hid::UsbHidClass;
use log::warn;
use usb_device::class_prelude::*;
use usb_device::Result;

/// HidMouse provides an interface to send mouse movement and button presses to
/// the host device
pub trait HidMouse {
    /// Writes an input report given representing the update to the mouse state
    /// to the host system
    fn write_mouse_report(&self, buttons: u8, x: i8, y: i8) -> Result<()>;
}

impl<B: UsbBus> HidMouse for UsbHidClass<'_, B> {
    fn write_mouse_report(
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
