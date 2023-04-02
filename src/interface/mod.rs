//! Abstract Human Interface Device Interfaces
use frunk::{HCons, HNil, ToRef};
use packed_struct::prelude::*;
use usb_device::bus::{InterfaceNumber, StringIndex, UsbBus, UsbBusAllocator};
use usb_device::class_prelude::DescriptorWriter;

use crate::hid_class::descriptor::{
    DescriptorType, HidProtocol, COUNTRY_CODE_NOT_SUPPORTED, SPEC_VERSION_1_11,
};

pub mod managed;
pub mod raw;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PackedStruct)]
#[packed_struct(endian = "lsb", size_bytes = 7)]
pub struct HidDescriptorBody {
    bcd_hid: u16,
    country_code: u8,
    num_descriptors: u8,
    #[packed_field(ty = "enum", size_bytes = "1")]
    descriptor_type: DescriptorType,
    descriptor_length: u16,
}

pub trait UsbAllocatable<'a, B: UsbBus> {
    type Allocated;
    fn allocate(self, usb_alloc: &'a UsbBusAllocator<B>) -> Self::Allocated;
}

impl<'a, B: UsbBus + 'a> UsbAllocatable<'a, B> for HNil {
    type Allocated = Self;

    fn allocate(self, _: &'a UsbBusAllocator<B>) -> Self::Allocated {
        self
    }
}

impl<'a, B, C, Tail> UsbAllocatable<'a, B> for HCons<C, Tail>
where
    B: UsbBus + 'a,
    C: UsbAllocatable<'a, B>,
    Tail: UsbAllocatable<'a, B>,
{
    type Allocated = HCons<C::Allocated, Tail::Allocated>;

    fn allocate(self, usb_alloc: &'a UsbBusAllocator<B>) -> Self::Allocated {
        HCons {
            head: self.head.allocate(usb_alloc),
            tail: self.tail.allocate(usb_alloc),
        }
    }
}

pub trait InterfaceClass<'a> {
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
    fn hid_descriptor_body(&self) -> [u8; 7] {
        let descriptor_len = u16::try_from(self.report_descriptor().len())
            .expect("Report descriptor too long, must be < u16::MAX");

        HidDescriptorBody {
            bcd_hid: SPEC_VERSION_1_11,
            country_code: COUNTRY_CODE_NOT_SUPPORTED,
            num_descriptors: 1,
            descriptor_type: DescriptorType::Report,
            descriptor_length: descriptor_len,
        }
        .pack()
        .map_err(drop) // Avoid pulling all the core::fmt code into final binary
        .expect("Failed to pack HidDescriptor")
    }
}

pub trait InterfaceHList<'a>: ToRef<'a> {
    fn get_id_mut(&mut self, id: u8) -> Option<&mut dyn InterfaceClass<'a>>;
    fn get_id(&self, id: u8) -> Option<&dyn InterfaceClass<'a>>;
    fn reset(&mut self);
    fn write_descriptors(&self, writer: &mut DescriptorWriter) -> usb_device::Result<()>;
    fn get_string(&self, index: StringIndex, lang_id: u16) -> Option<&'_ str>;
}

impl<'a> InterfaceHList<'a> for HNil {
    fn get_id_mut(&mut self, _: u8) -> Option<&mut dyn InterfaceClass<'a>> {
        None
    }

    fn get_id(&self, _: u8) -> Option<&dyn InterfaceClass<'a>> {
        None
    }

    fn reset(&mut self) {}

    fn write_descriptors(&self, _: &mut DescriptorWriter) -> usb_device::Result<()> {
        Ok(())
    }

    fn get_string(&self, _: StringIndex, _: u16) -> Option<&'static str> {
        None
    }
}

impl<'a, Head: InterfaceClass<'a> + 'a, Tail: InterfaceHList<'a>> InterfaceHList<'a>
    for HCons<Head, Tail>
{
    fn get_id_mut(&mut self, id: u8) -> Option<&mut dyn InterfaceClass<'a>> {
        if id == u8::from(self.head.id()) {
            Some(&mut self.head)
        } else {
            self.tail.get_id_mut(id)
        }
    }

    fn get_id(&self, id: u8) -> Option<&dyn InterfaceClass<'a>> {
        if id == u8::from(self.head.id()) {
            Some(&self.head)
        } else {
            self.tail.get_id(id)
        }
    }

    fn reset(&mut self) {
        self.head.reset();
        self.tail.reset();
    }

    fn write_descriptors(&self, writer: &mut DescriptorWriter) -> usb_device::Result<()> {
        self.head.write_descriptors(writer)?;
        self.tail.write_descriptors(writer)
    }

    fn get_string(&self, index: StringIndex, lang_id: u16) -> Option<&'_ str> {
        let s = self.head.get_string(index, lang_id);
        if s.is_some() {
            s
        } else {
            self.tail.get_string(index, lang_id)
        }
    }
}
