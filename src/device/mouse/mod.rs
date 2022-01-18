//!Implements Hid mouse devices
pub mod descriptor;

use crate::hid_class::prelude::*;
use embedded_time::duration::Milliseconds;
use log::warn;
use usb_device::class_prelude::*;
use usb_device::Result;

pub fn new_boot_mouse<B: usb_device::bus::UsbBus>(
    usb_alloc: &'_ UsbBusAllocator<B>,
) -> UsbHidClassBuilder<'_, B> {
    UsbHidClassBuilder::new(usb_alloc, descriptor::HID_BOOT_MOUSE_REPORT_DESCRIPTOR)
        .boot_device(InterfaceProtocol::Mouse)
        .interface_description("Mouse")
        .idle_default(Milliseconds(0))
        .unwrap()
        .in_endpoint(UsbPacketSize::Size8, Milliseconds(20))
        .unwrap()
        .without_out_endpoint()
}

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
