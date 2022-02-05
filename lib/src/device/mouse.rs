//!HID mice
use crate::hid_class::descriptor::HidProtocol;
use crate::hid_class::interface::{Interface, InterfaceClass, InterfaceConfig};
use delegate::delegate;
use embedded_time::duration::Milliseconds;
use frunk::{HCons, HNil};
use log::error;
use packed_struct::prelude::*;
use usb_device::bus::{InterfaceNumber, StringIndex, UsbBus};
use usb_device::class_prelude::DescriptorWriter;
use usb_device::Result;
use usb_device::UsbError;

use crate::hid_class::prelude::*;

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

#[derive(Clone, Copy, Debug, PartialEq, Default, PackedStruct)]
#[packed_struct(endian = "lsb", size_bytes = "3")]
pub struct BootMouseReport {
    #[packed_field]
    pub buttons: u8,
    #[packed_field]
    pub x: i8,
    #[packed_field]
    pub y: i8,
}

/// Create a pre-configured [`crate::hid_class::UsbHidClassBuilder`] for a boot mouse
pub fn new_boot_mouse<'a>() -> UsbHidClassBuilder<'a, HCons<InterfaceConfig<'a>, HNil>> {
    UsbHidClassBuilder::new().add_interface(
        InterfaceBuilder::new(BOOT_MOUSE_REPORT_DESCRIPTOR)
            .boot_device(InterfaceProtocol::Mouse)
            .description("Mouse")
            .idle_default(Milliseconds(0))
            .unwrap()
            .in_endpoint(UsbPacketSize::Bytes8, Milliseconds(10))
            .unwrap()
            .without_out_endpoint()
            .build(),
    )
}

/// Boot compatible mouse with wheel, pan and eight buttons
///
/// Reference: <https://docs.microsoft.com/en-us/previous-versions/windows/hardware/design/dn613912(v=vs.85)>
///            <https://www.microchip.com/forums/tm.aspx?m=391435>
#[rustfmt::skip]
pub const WHEEL_MOUSE_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x01,        // Usage Page (Generic Desktop),
    0x09, 0x02,        // Usage (Mouse),
    0xA1, 0x01,        // Collection (Application),
    0x09, 0x01,        //   Usage (Pointer),
    0xA1, 0x00,        //   Collection (Physical),
    0x95, 0x08,        //     Report Count (8),
    0x75, 0x01,        //     Report Size (1),
    0x05, 0x09,        //     Usage Page (Buttons),
    0x19, 0x01,        //     Usage Minimum (1),
    0x29, 0x08,        //     Usage Maximum (8),
    0x15, 0x00,        //     Logical Minimum (0),
    0x25, 0x01,        //     Logical Maximum (1),
    0x81, 0x02,        //     Input (Data, Variable, Absolute),
    0x75, 0x08,        //     Report Size (8),
    0x95, 0x02,        //     Report Count (2),
    0x05, 0x01,        //     Usage Page (Generic Desktop),
    0x09, 0x30,        //     Usage (X),
    0x09, 0x31,        //     Usage (Y),
    0x15, 0x81,        //     Logical Minimum (-127),
    0x25, 0x7F,        //     Logical Maximum (127),
    0x81, 0x06,        //     Input (Data, Variable, Relative),
    0x15, 0x81,        //     Logical Minimum (-127)
    0x25, 0x7F,        //     Logical Maximum (127)
    0x09, 0x38,        //     Usage (Wheel)
    0x75, 0x08,        //     Report Size (8)
    0x95, 0x01,        //     Report Count (1)
    0x81, 0x06,        //     Input (Data,Var,Rel,No Wrap,Linear,Preferred State,No Null Position)
    0x05, 0x0C,        //     Usage Page (Consumer)
    0x0A, 0x38, 0x02,  //     Usage (AC Pan)
    0x95, 0x01,        //     Report Count (1)
    0x81, 0x06,        //     Input (Data,Var,Rel,No Wrap,Linear,Preferred State,No Null Position)
    0xC0,              //   End Collection
    0xC0,              // End Collection
];

#[derive(Clone, Copy, Debug, PartialEq, Default, PackedStruct)]
#[packed_struct(endian = "lsb")]
pub struct WheelMouseReport {
    #[packed_field]
    pub buttons: u8,
    #[packed_field]
    pub x: i8,
    #[packed_field]
    pub y: i8,
    #[packed_field]
    pub vertical_wheel: i8,
    #[packed_field]
    pub horizontal_wheel: i8,
}

pub struct BootMouseInterface<'a, B: UsbBus> {
    inner: Interface<'a, B>,
}

impl<'a, B: UsbBus> BootMouseInterface<'a, B> {
    pub fn new(interface: Interface<'a, B>) -> Self {
        Self { inner: interface }
    }

    pub fn write_mouse_report(&self, report: BootMouseReport) -> usb_device::Result<usize> {
        let data = report.pack().map_err(|e| {
            error!("Error packing BootMouseReport: {:?}", e);
            UsbError::ParseError
        })?;
        self.inner.write_report(&data)
    }
}

impl<'a, B: UsbBus> InterfaceClass<'a> for BootMouseInterface<'a, B> {
    delegate! {
        to self.inner{
           fn report_descriptor(&self) -> &'a [u8];
           fn id(&self) -> InterfaceNumber;
           fn write_descriptors(&self, writer: &mut DescriptorWriter) -> usb_device::Result<()>;
           fn get_string(&self, index: StringIndex, _lang_id: u16) -> Option<&'static str>;
           fn reset(&mut self);
           fn set_report(&mut self, data: &[u8]) -> Result<()>;
           fn get_report(&mut self, data: &mut [u8]) -> Result<usize>;
           fn get_report_ack(&mut self) -> Result<()>;
           fn set_idle(&mut self, report_id: u8, value: u8);
           fn get_idle(&self, report_id: u8) -> u8;
           fn set_protocol(&mut self, protocol: HidProtocol);
           fn get_protocol(&self) -> HidProtocol;
        }
    }
}
