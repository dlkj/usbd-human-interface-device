//!HID consumer control

use fugit::ExtU32;
use packed_struct::prelude::*;
#[allow(clippy::wildcard_imports)]
use usb_device::class_prelude::*;

use crate::page::Consumer;
use crate::usb_class::prelude::*;

///Consumer control report descriptor - Four `u16` consumer control usage codes as an array (8 bytes)
#[rustfmt::skip]
pub const MULTIPLE_CODE_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x0C, // Usage Page (Consumer),
    0x09, 0x01, // Usage (Consumer Control),
    0xA1, 0x01, // Collection (Application),
    0x75, 0x10, //     Report Size(16)
    0x95, 0x04, //     Report Count(4)
    0x15, 0x00, //     Logical Minimum(0)
    0x26, 0x9C, 0x02, //     Logical Maximum(0x029C)
    0x19, 0x00, //     Usage Minimum(0)
    0x2A, 0x9C, 0x02, //     Usage Maximum(0x029C)
    0x81, 0x00, //     Input (Array, Data, Variable)
    0xC0, // End Collection
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default, PackedStruct)]
#[packed_struct(endian = "lsb", size_bytes = "8")]
pub struct MultipleConsumerReport {
    #[packed_field(ty = "enum", element_size_bytes = "2")]
    pub codes: [Consumer; 4],
}

#[allow(clippy::doc_markdown)]
///Fixed functionality consumer control report descriptor
/// 
/// Based on [Logitech Gaming Keyboard](http://www.usblyzer.com/reports/usb-properties/usb-keyboard.html)
/// dumped by [USBlyzer](http://www.usblyzer.com/)
/// 
/// Single bit packed `u8` report
/// * Bit 0 - Scan Next Track
/// * Bit 1 - Scan Previous Track
/// * Bit 2 - Stop
/// * Bit 3 - Play/Pause
/// * Bit 4 - Mute
/// * Bit 5 - Volume Increment
/// * Bit 6 - Volume Decrement
/// * Bit 7 - Reserved
#[rustfmt::skip]
pub const FIXED_FUNCTION_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x0C, //        Usage Page (Consumer Devices)  
    0x09, 0x01, //        Usage (Consumer Control)  
    0xA1, 0x01, //        Collection (Application)  
    0x05, 0x0C, //            Usage Page (Consumer Devices)  
    0x15, 0x00, //            Logical Minimum (0)  
    0x25, 0x01, //            Logical Maximum (1)  
    0x75, 0x01, //            Report Size (1)  
    0x95, 0x07, //            Report Count (7)  
    0x09, 0xB5, //            Usage (Scan Next Track)  
    0x09, 0xB6, //            Usage (Scan Previous Track)  
    0x09, 0xB7, //            Usage (Stop)  
    0x09, 0xCD, //            Usage (Play/Pause)  
    0x09, 0xE2, //            Usage (Mute)  
    0x09, 0xE9, //            Usage (Volume Increment)  
    0x09, 0xEA, //            Usage (Volume Decrement)  
    0x81, 0x02, //            Input (Data,Var,Abs,NWrp,Lin,Pref,NNul,Bit)  
    0x95, 0x01, //            Report Count (1)  
    0x81, 0x01, //            Input (Const,Ary,Abs)  
    0xC0, //        End Collection
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, PackedStruct)]
#[packed_struct(endian = "lsb", bit_numbering = "lsb0", size_bytes = "1")]
pub struct FixedFunctionReport {
    #[packed_field(bits = "0")]
    pub next: bool,
    #[packed_field(bits = "1")]
    pub previous: bool,
    #[packed_field(bits = "2")]
    pub stop: bool,
    #[packed_field(bits = "3")]
    pub play_pause: bool,
    #[packed_field(bits = "4")]
    pub mute: bool,
    #[packed_field(bits = "5")]
    pub volume_increment: bool,
    #[packed_field(bits = "6")]
    pub volume_decrement: bool,
}

pub struct ConsumerControl<'a, B: UsbBus> {
    interface: Interface<'a, B, InBytes8, OutNone, ReportSingle>,
}

impl<'a, B: UsbBus> ConsumerControl<'a, B> {
    pub fn write_report(&mut self, report: &MultipleConsumerReport) -> usb_device::Result<usize> {
        let data = report.pack().map_err(|_| {
            error!("Error packing MultipleConsumerReport");
            UsbError::ParseError
        })?;
        self.interface.write_report(&data)
    }
}

impl<'a, B: UsbBus> DeviceClass<'a> for ConsumerControl<'a, B> {
    type I = Interface<'a, B, InBytes8, OutNone, ReportSingle>;

    fn interface(&mut self) -> &mut Self::I {
        &mut self.interface
    }

    fn reset(&mut self) {}

    fn tick(&mut self) -> Result<(), crate::UsbHidError> {
        Ok(())
    }
}

pub struct ConsumerControlConfig<'a> {
    interface: InterfaceConfig<'a, InBytes8, OutNone, ReportSingle>,
}

impl<'a> ConsumerControlConfig<'a> {
    #[must_use]
    pub fn new(interface: InterfaceConfig<'a, InBytes8, OutNone, ReportSingle>) -> Self {
        Self { interface }
    }
}

impl<'a> Default for ConsumerControlConfig<'a> {
    #[must_use]
    fn default() -> Self {
        Self::new(
            unwrap!(
                unwrap!(InterfaceBuilder::new(MULTIPLE_CODE_REPORT_DESCRIPTOR))
                    .description("Consumer Control")
                    .in_endpoint(50.millis())
            )
            .without_out_endpoint()
            .build(),
        )
    }
}

impl<'a, B: UsbBus + 'a> UsbAllocatable<'a, B> for ConsumerControlConfig<'a> {
    type Allocated = ConsumerControl<'a, B>;

    fn allocate(self, usb_alloc: &'a UsbBusAllocator<B>) -> Self::Allocated {
        Self::Allocated {
            interface: Interface::new(usb_alloc, self.interface),
        }
    }
}

pub struct ConsumerControlFixed<'a, B: UsbBus> {
    interface: Interface<'a, B, InBytes8, OutNone, ReportSingle>,
}

impl<'a, B: UsbBus> ConsumerControlFixed<'a, B> {
    pub fn write_report(&mut self, report: &FixedFunctionReport) -> usb_device::Result<usize> {
        let data = report.pack().map_err(|_| {
            error!("Error packing MultipleConsumerReport");
            UsbError::ParseError
        })?;
        self.interface.write_report(&data)
    }
}

impl<'a, B: UsbBus> DeviceClass<'a> for ConsumerControlFixed<'a, B> {
    type I = Interface<'a, B, InBytes8, OutNone, ReportSingle>;

    fn interface(&mut self) -> &mut Self::I {
        &mut self.interface
    }

    fn reset(&mut self) {}

    fn tick(&mut self) -> Result<(), crate::UsbHidError> {
        Ok(())
    }
}

pub struct ConsumerControlFixedConfig<'a> {
    interface: InterfaceConfig<'a, InBytes8, OutNone, ReportSingle>,
}
impl<'a> ConsumerControlFixedConfig<'a> {
    #[must_use]
    pub fn new(interface: InterfaceConfig<'a, InBytes8, OutNone, ReportSingle>) -> Self {
        Self { interface }
    }
}

impl<'a> Default for ConsumerControlFixedConfig<'a> {
    #[must_use]
    fn default() -> Self {
        Self::new(
            unwrap!(
                unwrap!(InterfaceBuilder::new(FIXED_FUNCTION_REPORT_DESCRIPTOR))
                    .description("Consumer Control")
                    .in_endpoint(50.millis())
            )
            .without_out_endpoint()
            .build(),
        )
    }
}

impl<'a, B: UsbBus + 'a> UsbAllocatable<'a, B> for ConsumerControlFixedConfig<'a> {
    type Allocated = ConsumerControlFixed<'a, B>;

    fn allocate(self, usb_alloc: &'a UsbBusAllocator<B>) -> Self::Allocated {
        Self::Allocated {
            interface: Interface::new(usb_alloc, self.interface),
        }
    }
}
