//!HID consumer control devices

use delegate::delegate;
use fugit::ExtU32;
use log::error;
use packed_struct::prelude::*;
#[allow(clippy::wildcard_imports)]
use usb_device::class_prelude::*;
use usb_device::{Result, UsbError};

use crate::hid_class::prelude::*;
use crate::interface::raw::{RawInterface, RawInterfaceConfig};
use crate::interface::{InterfaceClass, WrappedInterface, WrappedInterfaceConfig};
use crate::page::Consumer;

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

pub struct ConsumerControlInterface<'a, B: UsbBus> {
    inner: RawInterface<'a, B>,
}

impl<'a, B: UsbBus> ConsumerControlInterface<'a, B> {
    pub fn write_report(&self, report: &MultipleConsumerReport) -> usb_device::Result<usize> {
        let data = report.pack().map_err(|e| {
            error!("Error packing MultipleConsumerReport: {:?}", e);
            UsbError::ParseError
        })?;
        self.inner.write_report(&data)
    }

    #[must_use]
    pub fn default_config() -> WrappedInterfaceConfig<Self, RawInterfaceConfig<'a>> {
        WrappedInterfaceConfig::new(
            RawInterfaceBuilder::new(MULTIPLE_CODE_REPORT_DESCRIPTOR)
                .description("Consumer Control")
                .in_endpoint(UsbPacketSize::Bytes8, 50.millis())
                .unwrap()
                .without_out_endpoint()
                .build(),
            (),
        )
    }
}

impl<'a, B: UsbBus> InterfaceClass<'a, B> for ConsumerControlInterface<'a, B> {
    #![allow(clippy::inline_always)]
    delegate! {
        to self.inner{
           fn report_descriptor(&self) -> &'_ [u8];
           fn id(&self) -> InterfaceNumber;
           fn write_descriptors(&self, writer: &mut DescriptorWriter) -> usb_device::Result<()>;
           fn get_string(&self, index: StringIndex, lang_id: u16) -> Option<&'_ str>;
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

    fn interface(&self) -> &RawInterface<'a, B> {
        &self.inner
    }
}

impl<'a, B: UsbBus> WrappedInterface<'a, B, RawInterface<'a, B>>
    for ConsumerControlInterface<'a, B>
{
    fn new(interface: RawInterface<'a, B>, _: ()) -> Self {
        Self { inner: interface }
    }
}

pub struct ConsumerControlFixedInterface<'a, B: UsbBus> {
    inner: RawInterface<'a, B>,
}

impl<'a, B: UsbBus> ConsumerControlFixedInterface<'a, B> {
    pub fn write_report(&self, report: &FixedFunctionReport) -> usb_device::Result<usize> {
        let data = report.pack().map_err(|e| {
            error!("Error packing MultipleConsumerReport: {:?}", e);
            UsbError::ParseError
        })?;
        self.inner.write_report(&data)
    }

    #[must_use]
    pub fn default_config() -> WrappedInterfaceConfig<Self, RawInterfaceConfig<'a>> {
        WrappedInterfaceConfig::new(
            RawInterfaceBuilder::new(FIXED_FUNCTION_REPORT_DESCRIPTOR)
                .description("Consumer Control")
                .in_endpoint(UsbPacketSize::Bytes8, 50.millis())
                .unwrap()
                .without_out_endpoint()
                .build(),
            (),
        )
    }
}

impl<'a, B: UsbBus> InterfaceClass<'a, B> for ConsumerControlFixedInterface<'a, B> {
    #![allow(clippy::inline_always)]
    delegate! {
        to self.inner{
           fn report_descriptor(&self) -> &'_ [u8];
           fn id(&self) -> InterfaceNumber;
           fn write_descriptors(&self, writer: &mut DescriptorWriter) -> usb_device::Result<()>;
           fn get_string(&self, index: StringIndex, lang_id: u16) -> Option<&'_ str>;
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

    fn interface(&self) -> &RawInterface<'a, B> {
        &self.inner
    }
}

impl<'a, B: UsbBus> WrappedInterface<'a, B, RawInterface<'a, B>>
    for ConsumerControlFixedInterface<'a, B>
{
    fn new(interface: RawInterface<'a, B>, _: ()) -> Self {
        Self { inner: interface }
    }
}
