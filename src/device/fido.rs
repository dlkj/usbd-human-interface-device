//! HID FIDO
use crate::hid_class::descriptor::HidProtocol;
use delegate::delegate;
use embedded_time::duration::Milliseconds;
use usb_device::bus::{InterfaceNumber, StringIndex, UsbBus};
use usb_device::class_prelude::DescriptorWriter;

use crate::hid_class::prelude::*;
use crate::interface::raw::{RawInterface, RawInterfaceConfig};
use crate::interface::{InterfaceClass, WrappedInterface, WrappedInterfaceConfig};
use crate::UsbHidError;
use core::ops::{Deref, DerefMut};

/// HID Mouse report descriptor conforming to the Boot specification
///
/// This aims to be compatible with BIOS and other reduced functionality USB hosts
///
/// This is defined in Appendix B.2 & E.10 of [Device Class Definition for Human
/// Interface Devices (Hid) Version 1.11](<https://www.usb.org/sites/default/files/hid1_11.pdf>)
#[rustfmt::skip]
pub const FIDO_REPORT_DESCRIPTOR: &[u8] = &[
    0x06, 0xD0, 0xF1, // Usage Page (FIDO),
    0x09, 0x01, // Usage (Mouse),
    0xA1, 0x01, // Collection (Application),
    0x09, 0x20, //   Usage (Data In),
    0x15, 0x00, //       Logical Minimum(0),
    0x26, 0xFF, 0x00, // Logical Maxs (0x00FF),
    0x75, 0x08, //       Report size (8)
    0x95, 0x40, //       Report count (64)
    0x81, 0x02, //       Input (Data | Variable | Absolute)
    0x09, 0x21, //   Usage (Data Out),
    0x15, 0x00, //       Logical Minimum(0),
    0x26, 0xFF, 0x00, // Logical Maxs (0x00FF),
    0x75, 0x08, //       Report size (8)
    0x95, 0x40, //       Report count (64)
    0x91, 0x02, //       Output (Data | Variable | Absolute)
    0xC0,       // End Collection
];

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C, align(8))]
pub struct FidoMsg {
    pub cmd: u8,
    pub bcnt: u8,
    pub data: [u8; 62],
}
impl Default for FidoMsg {
    fn default() -> FidoMsg {
        FidoMsg { cmd: 0, bcnt: 0, data: [0u8; 62] }
    }
}
impl Deref for FidoMsg {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(
                self as *const FidoMsg as *const u8, core::mem::size_of::<FidoMsg>()
            ) as &[u8]
        }
    }
}
impl DerefMut for FidoMsg {
    fn deref_mut(&mut self) -> &mut [u8] {
        unsafe {
            core::slice::from_raw_parts_mut(
                self as *mut FidoMsg as *mut u8, core::mem::size_of::<FidoMsg>())
                as &mut [u8]
        }
    }
}


pub struct FidoInterface<'a, B: UsbBus> {
    inner: RawInterface<'a, B>,
}

impl<'a, B: UsbBus> FidoInterface<'a, B> {
    pub fn write_report(&self, report: &FidoMsg) -> Result<(), UsbHidError> {
        self.inner
            .write_report(report.deref())
            .map(|_| ())
            .map_err(UsbHidError::from)
    }
    pub fn read_report(&self) -> usb_device::Result<FidoMsg> {
        let mut report = FidoMsg::default();
        match self.inner.read_report(&mut report.deref_mut()) {
            Err(e) => Err(e),
            Ok(_) => Ok(report),
        }
    }

    pub fn default_config() -> WrappedInterfaceConfig<Self, RawInterfaceConfig<'a>> {
        WrappedInterfaceConfig::new(
            RawInterfaceBuilder::new(FIDO_REPORT_DESCRIPTOR)
                .description("U2F Token")
                .idle_default(Milliseconds(0))
                .unwrap()
                .in_endpoint(UsbPacketSize::Bytes64, Milliseconds(5))
                .unwrap()
                .with_out_endpoint(UsbPacketSize::Bytes64, Milliseconds(5))
                .unwrap()
            .build(),
            (),
        )
    }
}

impl<'a, B: UsbBus> InterfaceClass<'a> for FidoInterface<'a, B> {
    delegate! {
        to self.inner{
           fn report_descriptor(&self) -> &'_ [u8];
           fn id(&self) -> InterfaceNumber;
           fn write_descriptors(&self, writer: &mut DescriptorWriter) -> usb_device::Result<()>;
           fn get_string(&self, index: StringIndex, _lang_id: u16) -> Option<&'_ str>;
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

impl<'a, B: UsbBus> WrappedInterface<'a, B, RawInterface<'a, B>> for FidoInterface<'a, B> {
    fn new(interface: RawInterface<'a, B>, _: ()) -> Self {
        Self { inner: interface }
    }
}
