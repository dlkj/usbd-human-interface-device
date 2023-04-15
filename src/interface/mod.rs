//! Abstract Human Interface Device Interfaces
use frunk::{HCons, HNil, ToMut};
use packed_struct::prelude::*;
use usb_device::bus::{StringIndex, UsbBus, UsbBusAllocator};
use usb_device::class_prelude::{DescriptorWriter, InterfaceNumber};

use crate::hid_class::descriptor::{DescriptorType, HidProtocol};
use crate::UsbHidError;

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

pub trait RawInterfaceT<'a, B> {
    fn hid_descriptor_body(&self) -> [u8; 7];
    fn report_descriptor(&self) -> &'_ [u8];
    fn id(&self) -> InterfaceNumber;
    fn write_descriptors(&self, writer: &mut DescriptorWriter) -> usb_device::Result<()>;
    fn get_string(&self, index: StringIndex, _lang_id: u16) -> Option<&'a str>;
    fn reset(&mut self);
    fn set_report(&mut self, data: &[u8]) -> usb_device::Result<()>;
    fn get_report(&self, data: &mut [u8]) -> usb_device::Result<usize>;
    fn get_report_ack(&mut self) -> usb_device::Result<()>;
    fn set_idle(&mut self, report_id: u8, value: u8);
    fn get_idle(&self, report_id: u8) -> u8;
    fn set_protocol(&mut self, protocol: HidProtocol);
    fn get_protocol(&self) -> HidProtocol;
}

pub trait InterfaceClass<'a, B: UsbBus> {
    type I: RawInterfaceT<'a, B>;
    fn interface(&mut self) -> &mut Self::I;
    /// Called if the USB Device is reset
    fn reset(&mut self);
    /// Called every 1ms
    fn tick(&mut self) -> Result<(), UsbHidError>;
}

pub trait InterfaceHList<'a, B>: ToMut<'a> {
    fn get(&mut self, id: u8) -> Option<&mut dyn RawInterfaceT<'a, B>>;
    fn reset(&mut self);
    fn write_descriptors(&mut self, writer: &mut DescriptorWriter) -> usb_device::Result<()>;
    fn get_string(&mut self, index: StringIndex, lang_id: u16) -> Option<&'a str>;
    fn tick(&mut self) -> Result<(), UsbHidError>;
}

impl<'a, B> InterfaceHList<'a, B> for HNil {
    fn get(&mut self, _: u8) -> Option<&mut dyn RawInterfaceT<'a, B>> {
        None
    }

    fn reset(&mut self) {}

    fn write_descriptors(&mut self, _: &mut DescriptorWriter) -> usb_device::Result<()> {
        Ok(())
    }

    fn get_string(&mut self, _: StringIndex, _: u16) -> Option<&'a str> {
        None
    }

    fn tick(&mut self) -> Result<(), UsbHidError> {
        Ok(())
    }
}

impl<'a, B: UsbBus + 'a, Head: InterfaceClass<'a, B> + 'a, Tail: InterfaceHList<'a, B>>
    InterfaceHList<'a, B> for HCons<Head, Tail>
{
    fn get(&mut self, id: u8) -> Option<&mut dyn RawInterfaceT<'a, B>> {
        if id == u8::from(self.head.interface().id()) {
            Some(self.head.interface())
        } else {
            self.tail.get(id)
        }
    }

    fn reset(&mut self) {
        self.head.interface().reset();
        self.head.reset();
        self.tail.reset();
    }

    fn write_descriptors(&mut self, writer: &mut DescriptorWriter) -> usb_device::Result<()> {
        self.head.interface().write_descriptors(writer)?;
        self.tail.write_descriptors(writer)
    }

    fn get_string(&mut self, index: StringIndex, lang_id: u16) -> Option<&'a str> {
        let s = self.head.interface().get_string(index, lang_id);
        if s.is_some() {
            s
        } else {
            self.tail.get_string(index, lang_id)
        }
    }

    fn tick(&mut self) -> Result<(), UsbHidError> {
        self.head.tick()?;
        self.tail.tick()
    }
}
