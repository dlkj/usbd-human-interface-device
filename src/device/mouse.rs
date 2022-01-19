//!HID mice
use crate::hid_class::prelude::*;
use embedded_time::duration::Milliseconds;
use log::warn;
use usb_device::class_prelude::*;
use usb_device::Result;

/// HID Mouse report descriptor conforming to the Boot specification
/// 
/// This aims to be compatible with BIOS and other reduced functionality USB hosts
///
/// This is defined in Appendix B.2 & E.10 of [Device Class Definition for Human
/// Interface Devices (Hid) Version 1.11](<https://www.usb.org/sites/default/files/hid1_11.pdf>)
#[rustfmt::skip]
pub const BOOT_MOUSE_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x01, // Usage Page (Generic Desktop),
    0x09, 0x02, // Usage (Mouse),
    0xA1, 0x01, // Collection (Application),
    0x09, 0x01, //   Usage (Pointer),
    0xA1, 0x00, //   Collection (Physical),
    0x95, 0x03, //     Report Count (3),
    0x75, 0x01, //     Report Size (1),
    0x05, 0x09, //     Usage Page (Buttons),
    0x19, 0x01, //     Usage Minimum (1),
    0x29, 0x03, //     Usage Maximum (3),
    0x15, 0x00, //     Logical Minimum (0),
    0x25, 0x01, //     Logical Maximum (1),
    0x81, 0x02, //     Input (Data, Variable, Absolute),
    0x95, 0x01, //     Report Count (1),
    0x75, 0x05, //     Report Size (5),
    0x81, 0x01, //     Input (Constant),
    0x75, 0x08, //     Report Size (8),
    0x95, 0x02, //     Report Count (2),
    0x05, 0x01, //     Usage Page (Generic Desktop),
    0x09, 0x30, //     Usage (X),
    0x09, 0x31, //     Usage (Y),
    0x15, 0x81, //     Logical Minimum (-127),
    0x25, 0x7F, //     Logical Maximum (127),
    0x81, 0x06, //     Input (Data, Variable, Relative),
    0xC0, //   End Collection,
    0xC0, // End Collection
];

/// Create a pre-configured [`crate::hid_class::UsbHidClassBuilder`] for a boot mouse
pub fn new_boot_mouse<B: usb_device::bus::UsbBus>(
    usb_alloc: &'_ UsbBusAllocator<B>,
) -> UsbHidClassBuilder<'_, B> {
    UsbHidClassBuilder::new(usb_alloc, BOOT_MOUSE_REPORT_DESCRIPTOR)
        .boot_device(InterfaceProtocol::Mouse)
        .interface_description("Mouse")
        .idle_default(Milliseconds(0))
        .unwrap()
        .in_endpoint(UsbPacketSize::Size8, Milliseconds(20))
        .unwrap()
        .without_out_endpoint()
}

/// Provides an interface to send mouse movement and button presses to
/// the host device
pub trait HidMouse {
    /// Writes an input report to the host system representing the update to the
    /// mouse's state
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
