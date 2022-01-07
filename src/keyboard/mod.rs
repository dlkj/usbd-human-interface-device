//!Implements HID keyboard devices
mod descriptors;

use embedded_time::duration::*;
use usb_device::class_prelude::*;
use usb_device::Result;

const USB_CLASS_HID: u8 = 0x03;
const HID_CLASS_SPEC_1_11: [u8; 2] = [0x11, 0x01]; //1.11 in BCD
const HID_COUNTRY_CODE_NOT_SUPPORTED: u8 = 0x0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum HIDProtocol {
    None = 0x00,
    Keyboard = 0x01,
    _Mouse = 0x02,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HIDDescriptorType {
    Hid = 0x21,
    Report = 0x22,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum HIDSubclass {
    None = 0x00,
    Boot = 0x01,
}

/// HIDKeyboard provides an interface to send keycodes to the host device and
/// receive LED status information
pub trait HIDKeyboard {
    /// Writes an input report given representing keycodes to the host system
    fn write_keycodes<K>(keycodes: K) -> Result<()>
    where
        K: IntoIterator<Item = u8>;

    /// Read LED status from the host system
    fn read_leds() -> Result<u8>;
}

/// Implements a HID Keyboard that conforms to the Boot specification. This aims
/// to be compatible with BIOS and other reduced functionality USB hosts
///
/// This is defined in Appendix B.1 of Device Class Definition for Human
/// Interface Devices (HID) Version 1.11 -
/// https://www.usb.org/sites/default/files/hid1_11.pdf
pub struct HIDBootKeyboard<'a, B: UsbBus> {
    number: InterfaceNumber,
    out_endpoint: Option<EndpointOut<'a, B>>,
    in_endpoint: Option<EndpointIn<'a, B>>,
    report_descriptor: &'static [u8],
    string_index: StringIndex,
}

impl<B: UsbBus> HIDBootKeyboard<'_, B> {
    pub fn new<I: Into<Milliseconds>>(
        usb_alloc: &'_ UsbBusAllocator<B>,
        interval: I,
    ) -> HIDBootKeyboard<'_, B> {
        let interval_ms = interval.into().integer() as u8;

        HIDBootKeyboard {
            number: usb_alloc.interface(),
            //Report size must be 8 bytes or less, so also capping the packet size
            out_endpoint: Some(usb_alloc.interrupt(8, interval_ms)),
            in_endpoint: Some(usb_alloc.interrupt(8, interval_ms)),
            report_descriptor: &descriptors::HID_BOOT_KEYBOARD_REPORT_DESCRIPTOR,
            string_index: usb_alloc.string(),
        }
    }
}

impl<B: UsbBus> HIDKeyboard for HIDBootKeyboard<'_, B> {
    fn write_keycodes<K>(_: K) -> core::result::Result<(), usb_device::UsbError>
    where
        K: IntoIterator<Item = u8>,
    {
        todo!()
    }
    fn read_leds() -> core::result::Result<u8, usb_device::UsbError> {
        todo!()
    }
}

impl<B: UsbBus> UsbClass<B> for HIDBootKeyboard<'_, B> {
    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> Result<()> {
        writer.interface_alt(
            self.number,
            usb_device::device::DEFAULT_ALTERNATE_SETTING,
            USB_CLASS_HID,
            HIDSubclass::Boot as u8,
            HIDProtocol::Keyboard as u8,
            Some(self.string_index),
        )?;

        if self.report_descriptor.len() > u16::max_value() as usize {
            return Err(UsbError::InvalidState);
        }

        writer.write(
            HIDDescriptorType::Hid as u8,
            &[
                HID_CLASS_SPEC_1_11[0], //HID Class spec version 1.10 (BCD)
                HID_CLASS_SPEC_1_11[1],
                HID_COUNTRY_CODE_NOT_SUPPORTED, //Country code 0 - not supported
                1,                              //Num descriptors - 1
                HIDDescriptorType::Report as u8, //Class descriptor type is report
                (self.report_descriptor.len() & 0xFF) as u8, //Descriptor length
                (self.report_descriptor.len() >> 8 & 0xFF) as u8,
            ],
        )?;

        if let Some(ep) = &self.out_endpoint {
            writer.endpoint(ep)?;
        }
        if let Some(ep) = &self.in_endpoint {
            writer.endpoint(ep)?;
        }

        Ok(())
    }

    fn get_string(&self, index: StringIndex, lang_id: u16) -> Option<&str> {
        let _ = lang_id;

        if index == self.string_index {
            Some("Keyboard")
        } else {
            None
        }
    }

    fn control_in(&mut self, xfer: ControlIn<B>) {
        let _ = xfer;
        todo!()
    }

    fn control_out(&mut self, xfer: ControlOut<B>) {
        let _ = xfer;
        todo!()
    }
}
