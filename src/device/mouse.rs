//!HID mice
use crate::usb_class::prelude::*;
use core::default::Default;
use fugit::ExtU32;
use packed_struct::prelude::*;
use usb_device::bus::UsbBus;
use usb_device::class_prelude::UsbBusAllocator;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default, PackedStruct)]
#[packed_struct(endian = "lsb", size_bytes = "3")]
pub struct BootMouseReport {
    #[packed_field]
    pub buttons: u8,
    #[packed_field]
    pub x: i8,
    #[packed_field]
    pub y: i8,
}

/// Boot compatible mouse with wheel, pan and eight buttons
///
/// Reference: <https://docs.microsoft.com/en-us/previous-versions/windows/hardware/design/dn613912(v=vs.85)>
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default, PackedStruct)]
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

/// Absolute mouse with wheel and eight buttons
///
/// Note - absolute pointer support is relatively uncommon. This has been tested on Windows 11
/// Other operating systems may not natively support this device.
/// 
/// Windows only natively supports absolute pointer devices on the primary display.
/// 
/// Reference: <https://docs.microsoft.com/en-us/previous-versions/windows/hardware/design/dn613912(v=vs.85)>
#[rustfmt::skip]
pub const ABSOLUTE_WHEEL_MOUSE_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x01,        // Usage Page (Generic Desktop),
    0x09, 0x02,        // Usage (Mouse),
    0xA1, 0x01,        // Collection (Application),
    0x09, 0x01,        //   Usage (Pointer),
    0xA1, 0x00,        //   Collection (Physical),

    0x05, 0x09,        //     Usage Page (Buttons),
    0x19, 0x01,        //     Usage Minimum (1),
    0x29, 0x08,        //     Usage Maximum (8),
    0x15, 0x00,        //     Logical Minimum (0),
    0x25, 0x01,        //     Logical Maximum (1),
    0x95, 0x08,        //     Report Count (8),
    0x75, 0x01,        //     Report Size (1),
    0x81, 0x02,        //     Input (Data, Variable, Absolute),

    0x05, 0x01,        //     Usage Page (Generic Desktop),
    0x09, 0x30,        //     Usage (X),
    0x09, 0x31,        //     Usage (Y),
    0x15, 0x00,        //     Logical Minimum (0),
    0x26, 0xFF, 0x7F,  //     Logical Maximum (32767),
    0x35, 0x00,        //     Physical Minimum (0),
    0x46, 0xFF, 0x7F,  //     Physical Maximum (32767),
    0x95, 0x02,        //     Report Count (2),
    0x75, 0x10,        //     Report Size (16),
    0x81, 0x02,        //     Input (Data, Variable, Absolute),

    0x09, 0x38,        //     Usage (Wheel)
    0x15, 0x81,        //     Logical Minimum (-127)
    0x25, 0x7F,        //     Logical Maximum (127)
    0x35, 0x81,        //     Physical Minimum (-127),
    0x45, 0x7F,        //     Physical Maximum (127),
    0x75, 0x08,        //     Report Size (8)
    0x95, 0x01,        //     Report Count (1)
    0x81, 0x06,        //     Input (Data,Var,Rel,No Wrap,Linear,Preferred State,No Null Position)

    0xC0,              //   End Collection
    0xC0,              // End Collection
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default, PackedStruct)]
#[packed_struct(endian = "lsb")]
pub struct AbsoluteWheelMouseReport {
    #[packed_field]
    pub buttons: u8,
    #[packed_field]
    pub x: u16,
    #[packed_field]
    pub y: u16,
    #[packed_field]
    pub wheel: i8,
}

pub struct BootMouse<'a, B: UsbBus> {
    interface: Interface<'a, B, InBytes8, OutNone, ReportSingle>,
}

impl<'a, B: UsbBus> BootMouse<'a, B> {
    pub fn write_report(&mut self, report: &BootMouseReport) -> Result<(), UsbHidError> {
        let data = report.pack().map_err(|_| {
            error!("Error packing BootMouseReport");
            UsbHidError::SerializationError
        })?;
        self.interface
            .write_report(&data)
            .map(|_| ())
            .map_err(UsbHidError::from)
    }
}

pub struct BootMouseConfig<'a> {
    interface: InterfaceConfig<'a, InBytes8, OutNone, ReportSingle>,
}

impl<'a> BootMouseConfig<'a> {
    #[must_use]
    pub fn new(interface: InterfaceConfig<'a, InBytes8, OutNone, ReportSingle>) -> Self {
        Self { interface }
    }
}

impl<'a> Default for BootMouseConfig<'a> {
    #[must_use]
    fn default() -> Self {
        Self::new(
            unwrap!(unwrap!(InterfaceBuilder::new(BOOT_MOUSE_REPORT_DESCRIPTOR))
                .boot_device(InterfaceProtocol::Mouse)
                .description("Mouse")
                .in_endpoint(10.millis()))
            .without_out_endpoint()
            .build(),
        )
    }
}

impl<'a, B: UsbBus + 'a> UsbAllocatable<'a, B> for BootMouseConfig<'a> {
    type Allocated = BootMouse<'a, B>;

    fn allocate(self, usb_alloc: &'a UsbBusAllocator<B>) -> Self::Allocated {
        BootMouse {
            interface: self.interface.allocate(usb_alloc),
        }
    }
}

impl<'a, B: UsbBus> DeviceClass<'a> for BootMouse<'a, B> {
    type I = Interface<'a, B, InBytes8, OutNone, ReportSingle>;

    fn interface(&mut self) -> &mut Self::I {
        &mut self.interface
    }

    fn reset(&mut self) {}

    fn tick(&mut self) -> Result<(), UsbHidError> {
        Ok(())
    }
}

pub struct WheelMouse<'a, B: UsbBus> {
    interface: Interface<'a, B, InBytes8, OutNone, ReportSingle>,
}

impl<'a, B: UsbBus> WheelMouse<'a, B> {
    pub fn write_report(&mut self, report: &WheelMouseReport) -> Result<(), UsbHidError> {
        let data = report.pack().map_err(|_| {
            error!("Error packing WheelMouseReport");
            UsbHidError::SerializationError
        })?;
        self.interface
            .write_report(&data)
            .map(|_| ())
            .map_err(UsbHidError::from)
    }
}
pub struct WheelMouseConfig<'a> {
    interface: InterfaceConfig<'a, InBytes8, OutNone, ReportSingle>,
}

impl<'a> WheelMouseConfig<'a> {
    #[must_use]
    pub fn new(interface: InterfaceConfig<'a, InBytes8, OutNone, ReportSingle>) -> Self {
        Self { interface }
    }
}

impl<'a> Default for WheelMouseConfig<'a> {
    #[must_use]
    fn default() -> Self {
        WheelMouseConfig::new(
            unwrap!(
                unwrap!(InterfaceBuilder::new(WHEEL_MOUSE_REPORT_DESCRIPTOR))
                    .boot_device(InterfaceProtocol::Mouse)
                    .description("Wheel Mouse")
                    .in_endpoint(10.millis())
            )
            .without_out_endpoint()
            .build(),
        )
    }
}

impl<'a, B: UsbBus + 'a> UsbAllocatable<'a, B> for WheelMouseConfig<'a> {
    type Allocated = WheelMouse<'a, B>;

    fn allocate(self, usb_alloc: &'a UsbBusAllocator<B>) -> Self::Allocated {
        WheelMouse {
            interface: self.interface.allocate(usb_alloc),
        }
    }
}

impl<'a, B: UsbBus> DeviceClass<'a> for WheelMouse<'a, B> {
    type I = Interface<'a, B, InBytes8, OutNone, ReportSingle>;

    fn interface(&mut self) -> &mut Self::I {
        &mut self.interface
    }

    fn reset(&mut self) {}

    fn tick(&mut self) -> Result<(), UsbHidError> {
        Ok(())
    }
}

pub struct AbsoluteWheelMouse<'a, B: UsbBus> {
    interface: Interface<'a, B, InBytes8, OutNone, ReportSingle>,
}

impl<'a, B: UsbBus> AbsoluteWheelMouse<'a, B> {
    pub fn write_report(&mut self, report: &AbsoluteWheelMouseReport) -> Result<(), UsbHidError> {
        let data = report.pack().map_err(|_| {
            error!("Error packing WheelMouseReport");
            UsbHidError::SerializationError
        })?;
        self.interface
            .write_report(&data)
            .map(|_| ())
            .map_err(UsbHidError::from)
    }
}

pub struct AbsoluteWheelMouseConfig<'a> {
    interface: InterfaceConfig<'a, InBytes8, OutNone, ReportSingle>,
}

impl<'a> AbsoluteWheelMouseConfig<'a> {
    #[must_use]
    pub fn new(interface: InterfaceConfig<'a, InBytes8, OutNone, ReportSingle>) -> Self {
        Self { interface }
    }
}

impl<'a> Default for AbsoluteWheelMouseConfig<'a> {
    #[must_use]
    fn default() -> Self {
        AbsoluteWheelMouseConfig::new(
            unwrap!(unwrap!(InterfaceBuilder::new(
                ABSOLUTE_WHEEL_MOUSE_REPORT_DESCRIPTOR
            ))
            .description("Absolute Wheel Mouse")
            .in_endpoint(10.millis()))
            .without_out_endpoint()
            .build(),
        )
    }
}

impl<'a, B: UsbBus + 'a> UsbAllocatable<'a, B> for AbsoluteWheelMouseConfig<'a> {
    type Allocated = AbsoluteWheelMouse<'a, B>;

    fn allocate(self, usb_alloc: &'a UsbBusAllocator<B>) -> Self::Allocated {
        AbsoluteWheelMouse {
            interface: self.interface.allocate(usb_alloc),
        }
    }
}

impl<'a, B: UsbBus> DeviceClass<'a> for AbsoluteWheelMouse<'a, B> {
    type I = Interface<'a, B, InBytes8, OutNone, ReportSingle>;

    fn interface(&mut self) -> &mut Self::I {
        &mut self.interface
    }

    fn reset(&mut self) {}

    fn tick(&mut self) -> Result<(), UsbHidError> {
        Ok(())
    }
}
