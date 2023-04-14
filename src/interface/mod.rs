//! Abstract Human Interface Device Interfaces
use frunk::{HCons, HNil, ToMut};
use packed_struct::prelude::*;
use usb_device::bus::{StringIndex, UsbBus, UsbBusAllocator};
use usb_device::class_prelude::DescriptorWriter;

use crate::hid_class::descriptor::DescriptorType;

use self::raw::RawInterface;

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

pub trait InterfaceClass<'a, B: UsbBus> {
    fn interface(&mut self) -> &mut RawInterface<'a, B>;
    fn reset(&mut self);
}

pub trait InterfaceHList<'a, B>: ToMut<'a> {
    fn get(&mut self, id: u8) -> Option<&mut dyn InterfaceClass<'a, B>>;
    fn reset(&mut self);
    fn write_descriptors(&mut self, writer: &mut DescriptorWriter) -> usb_device::Result<()>;
    fn get_string(&mut self, index: StringIndex, lang_id: u16) -> Option<&'a str>;
}

impl<'a, B> InterfaceHList<'a, B> for HNil {
    fn get(&mut self, _: u8) -> Option<&mut dyn InterfaceClass<'a, B>> {
        None
    }

    fn reset(&mut self) {}

    fn write_descriptors(&mut self, _: &mut DescriptorWriter) -> usb_device::Result<()> {
        Ok(())
    }

    fn get_string(&mut self, _: StringIndex, _: u16) -> Option<&'a str> {
        None
    }
}

impl<'a, B: UsbBus + 'a, Head: InterfaceClass<'a, B> + 'a, Tail: InterfaceHList<'a, B>>
    InterfaceHList<'a, B> for HCons<Head, Tail>
{
    fn get(&mut self, id: u8) -> Option<&mut dyn InterfaceClass<'a, B>> {
        if id == u8::from(self.head.interface().id()) {
            Some(&mut self.head)
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
}
