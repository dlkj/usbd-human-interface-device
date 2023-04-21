//! Concrete implementation of Human Interface Devices

use crate::interface::InterfaceClass;
use crate::UsbHidError;
use frunk::{HCons, HNil, ToMut};
#[allow(clippy::wildcard_imports)]
use usb_device::class_prelude::*;

pub mod consumer;
pub mod fido;
pub mod joystick;
pub mod keyboard;
pub mod mouse;

pub trait DeviceClass<'a> {
    type I: InterfaceClass<'a>;
    fn interface(&mut self) -> &mut Self::I;
    /// Called if the USB Device is reset
    fn reset(&mut self);
    /// Called every 1ms
    fn tick(&mut self) -> Result<(), UsbHidError>;
}

pub trait DeviceHList<'a>: ToMut<'a> {
    fn get(&mut self, id: u8) -> Option<&mut dyn InterfaceClass<'a>>;
    fn reset(&mut self);
    fn write_descriptors(&mut self, writer: &mut DescriptorWriter) -> usb_device::Result<()>;
    fn get_string(&mut self, index: StringIndex, lang_id: u16) -> Option<&'a str>;
    fn tick(&mut self) -> Result<(), UsbHidError>;
}

impl<'a> DeviceHList<'a> for HNil {
    fn get(&mut self, _: u8) -> Option<&mut dyn InterfaceClass<'a>> {
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

impl<'a, Head: DeviceClass<'a> + 'a, Tail: DeviceHList<'a>> DeviceHList<'a> for HCons<Head, Tail> {
    fn get(&mut self, id: u8) -> Option<&mut dyn InterfaceClass<'a>> {
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
