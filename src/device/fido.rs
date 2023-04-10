//! HID FIDO Universal 2nd Factor (U2F)
use fugit::ExtU32;
use usb_device::bus::UsbBus;

use crate::hid_class::prelude::*;
use crate::interface::raw::{RawInterface, RawInterfaceConfig};
use crate::interface::{InterfaceClass, WrappedInterface, WrappedInterfaceConfig};
use crate::UsbHidError;

/// Raw FIDO report descriptor.
/// 
/// See the [FIDO U2F HID Protocol Specification](https://fidoalliance.org/specs/fido-u2f-v1.2-ps-20170411/fido-u2f-hid-protocol-v1.2-ps-20170411.html)
/// for protocol detail
#[rustfmt::skip]
pub const FIDO_REPORT_DESCRIPTOR: &[u8] = &[
    0x06, 0xD0, 0xF1, // Usage Page (FIDO),
    0x09, 0x01, // Usage (U2F Authenticator Device)
    0xA1, 0x01, // Collection (Application),
    0x09, 0x20, //   Usage (Data In),
    0x15, 0x00, //       Logical Minimum(0),
    0x26, 0xFF, 0x00, // Logical Max (0x00FF),
    0x75, 0x08, //       Report size (8)
    0x95, 0x40, //       Report count (64)
    0x81, 0x02, //       Input (Data | Variable | Absolute)
    0x09, 0x21, //   Usage (Data Out),
    0x15, 0x00, //       Logical Minimum(0),
    0x26, 0xFF, 0x00, // Logical Max (0x00FF),
    0x75, 0x08, //       Report size (8)
    0x95, 0x40, //       Report count (64)
    0x91, 0x02, //       Output (Data | Variable | Absolute)
    0xC0,       // End Collection
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C, align(8))]
pub struct RawFidoMsg {
    pub packet: [u8; 64],
}
impl Default for RawFidoMsg {
    fn default() -> RawFidoMsg {
        RawFidoMsg { packet: [0u8; 64] }
    }
}

pub struct RawFidoInterface<'a, B: UsbBus> {
    inner: RawInterface<'a, B>,
}

impl<'a, B: UsbBus> RawFidoInterface<'a, B> {
    pub fn write_report(&mut self, report: &RawFidoMsg) -> Result<(), UsbHidError> {
        self.inner
            .write_report(&report.packet)
            .map(|_| ())
            .map_err(UsbHidError::from)
    }
    pub fn read_report(&mut self) -> usb_device::Result<RawFidoMsg> {
        let mut report = RawFidoMsg::default();
        match self.inner.read_report(&mut report.packet) {
            Err(e) => Err(e),
            Ok(_) => Ok(report),
        }
    }

    #[must_use]
    pub fn default_config() -> WrappedInterfaceConfig<Self, RawInterfaceConfig<'a>> {
        WrappedInterfaceConfig::new(
            RawInterfaceBuilder::new(FIDO_REPORT_DESCRIPTOR)
                .description("U2F Token")
                .in_endpoint(UsbPacketSize::Bytes64, 5.millis())
                .unwrap()
                .with_out_endpoint(UsbPacketSize::Bytes64, 5.millis())
                .unwrap()
                .build(),
            (),
        )
    }
}

impl<'a, B: UsbBus> InterfaceClass<'a, B> for RawFidoInterface<'a, B> {
    fn interface(&self) -> &RawInterface<'a, B> {
        &self.inner
    }

    fn interface_mut(&mut self) -> &mut RawInterface<'a, B> {
        &mut self.inner
    }

    fn reset(&mut self) {}
}

impl<'a, B: UsbBus> WrappedInterface<'a, B, RawInterface<'a, B>> for RawFidoInterface<'a, B> {
    fn new(interface: RawInterface<'a, B>, _: ()) -> Self {
        Self { inner: interface }
    }
}
