//! HID FIDO Universal 2nd Factor (U2F)
use crate::usb_class::prelude::*;
use fugit::ExtU32;
use usb_device::bus::UsbBus;
use usb_device::class_prelude::UsbBusAllocator;

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
pub struct RawFidoReport {
    pub packet: [u8; 64],
}
impl Default for RawFidoReport {
    fn default() -> Self {
        Self { packet: [0u8; 64] }
    }
}

pub struct RawFido<'a, B: UsbBus> {
    interface: Interface<'a, B, InBytes64, OutBytes64, ReportSingle>,
}

impl<'a, B: UsbBus> RawFido<'a, B> {
    pub fn write_report(&mut self, report: &RawFidoReport) -> Result<(), UsbHidError> {
        self.interface
            .write_report(&report.packet)
            .map(|_| ())
            .map_err(UsbHidError::from)
    }
    pub fn read_report(&mut self) -> usb_device::Result<RawFidoReport> {
        let mut report = RawFidoReport::default();
        match self.interface.read_report(&mut report.packet) {
            Err(e) => Err(e),
            Ok(_) => Ok(report),
        }
    }
}

impl<'a, B: UsbBus> DeviceClass<'a> for RawFido<'a, B> {
    type I = Interface<'a, B, InBytes64, OutBytes64, ReportSingle>;

    fn interface(&mut self) -> &mut Self::I {
        &mut self.interface
    }

    fn reset(&mut self) {}

    fn tick(&mut self) -> Result<(), UsbHidError> {
        Ok(())
    }
}

pub struct RawFidoConfig<'a> {
    interface: InterfaceConfig<'a, InBytes64, OutBytes64, ReportSingle>,
}

impl<'a> Default for RawFidoConfig<'a> {
    #[must_use]
    fn default() -> Self {
        Self::new(
            unwrap!(
                unwrap!(unwrap!(InterfaceBuilder::new(FIDO_REPORT_DESCRIPTOR))
                    .description("U2F Token")
                    .in_endpoint(5.millis()))
                .with_out_endpoint(5.millis())
            )
            .build(),
        )
    }
}

impl<'a> RawFidoConfig<'a> {
    #[must_use]
    pub fn new(interface: InterfaceConfig<'a, InBytes64, OutBytes64, ReportSingle>) -> Self {
        Self { interface }
    }
}

impl<'a, B: UsbBus + 'a> UsbAllocatable<'a, B> for RawFidoConfig<'a> {
    type Allocated = RawFido<'a, B>;

    fn allocate(self, usb_alloc: &'a UsbBusAllocator<B>) -> Self::Allocated {
        Self::Allocated {
            interface: Interface::new(usb_alloc, self.interface),
        }
    }
}
