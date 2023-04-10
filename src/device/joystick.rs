//!HID joystick
use core::default::Default;
use fugit::ExtU32;
use log::error;
use packed_struct::prelude::*;
use usb_device::bus::UsbBus;

use crate::hid_class::prelude::*;
use crate::interface::raw::{RawInterface, RawInterfaceConfig};
use crate::interface::{InterfaceClass, WrappedInterface, WrappedInterfaceConfig};
use crate::UsbHidError;

#[rustfmt::skip]
pub const JOYSTICK_DESCRIPTOR: &[u8] = &[
    0x05, 0x01, // Usage Page (Generic Desktop)         5,   1
    0x09, 0x04, // Usage (Joystick)                     9,   4
    0xa1, 0x01, // Collection (Application)             161, 1
    0x09, 0x01, //   Usage Page (Pointer)               9,   1
    0xa1, 0x00, //   Collection (Physical)              161, 0
    0x09, 0x30, //     Usage (X)                        9,   48
    0x09, 0x31, //     Usage (Y)                        9,   49
    0x15, 0x81, //     Logical Minimum (-127)           21,  129
    0x25, 0x7f, //     Logical Maximum (127)            37,  127
    0x75, 0x08, //     Report Size (8)                  117, 8
    0x95, 0x02, //     Report count (2)                 149, 2,
    0x81, 0x02, //     Input (Data, Variable, Absolute) 129, 2,
    0xc0,       //   End Collection                     192,
    0x05, 0x09, //   Usage Page (Button)                5,   9,
    0x19, 0x01, //   Usage Minimum (0)                  25,  1,
    0x29, 0x08, //   Usage Maximum (8)                  41,  8,
    0x15, 0x00, //   Logical Minimum (0)                21,  0
    0x25, 0x01, //   Logical Maximum (1)                37,  1,
    0x75, 0x01, //   Report Size (1)                    117, 1,
    0x95, 0x08, //   Report Count (8)                   149, 8
    0x81, 0x02, //   Input (Data, Variable, Absolute)   129, 2,
    0xc0,       // End Collection                       192
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default, PackedStruct)]
#[packed_struct(endian = "lsb", size_bytes = "3")]
pub struct JoystickReport {
    #[packed_field]
    pub x: i8,
    #[packed_field]
    pub y: i8,
    #[packed_field]
    pub buttons: u8,
}

pub struct JoystickInterface<'a, B: UsbBus> {
    inner: RawInterface<'a, B>,
}

impl<'a, B: UsbBus> JoystickInterface<'a, B> {
    pub fn write_report(&mut self, report: &JoystickReport) -> Result<(), UsbHidError> {
        let data = report.pack().map_err(|e| {
            error!("Error packing JoystickReport: {:?}", e);
            UsbHidError::SerializationError
        })?;
        self.inner
            .write_report(&data)
            .map(|_| ())
            .map_err(UsbHidError::from)
    }

    #[must_use]
    pub fn default_config() -> WrappedInterfaceConfig<Self, RawInterfaceConfig<'a>> {
        WrappedInterfaceConfig::new(
            RawInterfaceBuilder::new(JOYSTICK_DESCRIPTOR)
                .boot_device(InterfaceProtocol::None)
                .description("Joystick")
                .in_endpoint(UsbPacketSize::Bytes8, 10.millis())
                .unwrap()
                .without_out_endpoint()
                .build(),
            (),
        )
    }
}

impl<'a, B: UsbBus> InterfaceClass<'a, B> for JoystickInterface<'a, B> {
    fn interface(&mut self) -> &mut RawInterface<'a, B> {
        &mut self.inner
    }

    fn reset(&mut self) {}
}

impl<'a, B: UsbBus> WrappedInterface<'a, B, RawInterface<'a, B>> for JoystickInterface<'a, B> {
    fn new(interface: RawInterface<'a, B>, _: ()) -> Self {
        Self { inner: interface }
    }
}
