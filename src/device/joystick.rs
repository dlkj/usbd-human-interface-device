//!HID joystick
use crate::hid_class::descriptor::HidProtocol;
use core::default::Default;
use delegate::delegate;
use fugit::ExtU32;
use log::error;
use packed_struct::prelude::*;
use usb_device::bus::{InterfaceNumber, StringIndex, UsbBus};
use usb_device::class_prelude::{DescriptorWriter, UsbBusAllocator};

use crate::hid_class::prelude::*;
use crate::interface::raw::{RawInterface, RawInterfaceConfig};
use crate::interface::{InterfaceClass, UsbAllocatable};
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
    pub fn write_report(&self, report: &JoystickReport) -> Result<(), UsbHidError> {
        let data = report.pack().map_err(|e| {
            error!("Error packing JoystickReport: {:?}", e);
            UsbHidError::SerializationError
        })?;
        self.inner
            .write_report(&data)
            .map(|_| ())
            .map_err(UsbHidError::from)
    }
}

impl<'a, B: UsbBus> InterfaceClass<'a> for JoystickInterface<'a, B> {
    #![allow(clippy::inline_always)]
    delegate! {
        to self.inner{
           fn report_descriptor(&self) -> &'_ [u8];
           fn id(&self) -> InterfaceNumber;
           fn write_descriptors(&self, writer: &mut DescriptorWriter) -> usb_device::Result<()>;
           fn get_string(&self, index: StringIndex, lang_id: u16) -> Option<&'_ str>;
           fn reset(&mut self);
           fn set_report(&mut self, data: &[u8]) -> usb_device::Result<()>;
           fn get_report(&mut self, data: &mut [u8]) -> usb_device::Result<usize>;
           fn get_report_ack(&mut self) -> usb_device::Result<()>;
           fn set_idle(&mut self, report_id: u8, value: u8);
           fn get_idle(&self, report_id: u8) -> u8;
           fn set_protocol(&mut self, protocol: HidProtocol);
           fn get_protocol(&self) -> HidProtocol;
        }
    }
}

pub struct JoystickConfig<'a> {
    interface: RawInterfaceConfig<'a>,
}

impl<'a> Default for JoystickConfig<'a> {
    #[must_use]
    fn default() -> Self {
        Self::new(
            RawInterfaceBuilder::new(JOYSTICK_DESCRIPTOR)
                .boot_device(InterfaceProtocol::None)
                .description("Joystick")
                .in_endpoint(UsbPacketSize::Bytes8, 10.millis())
                .unwrap()
                .without_out_endpoint()
                .build(),
        )
    }
}

impl<'a> JoystickConfig<'a> {
    #[must_use]
    pub fn new(interface: RawInterfaceConfig<'a>) -> Self {
        Self { interface }
    }
}

impl<'a, B: UsbBus + 'a> UsbAllocatable<'a, B> for JoystickConfig<'a> {
    type Allocated = JoystickInterface<'a, B>;

    fn allocate(self, usb_alloc: &'a UsbBusAllocator<B>) -> Self::Allocated {
        Self::Allocated {
            inner: RawInterface::new(usb_alloc, self.interface),
        }
    }
}
