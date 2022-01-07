//!Implements HID keyboard devices
mod descriptors;

use embedded_time::duration::*;
use log::{debug, error, info, trace};
use usb_device::class_prelude::*;
use usb_device::Result;

const USB_CLASS_HID: u8 = 0x03;
const HID_CLASS_SPEC_1_11: [u8; 2] = [0x11, 0x01]; //1.11 in BCD
const HID_COUNTRY_CODE_NOT_SUPPORTED: u8 = 0x0;

const HID_REQ_SET_IDLE: u8 = 0x0a;
const HID_REQ_GET_IDLE: u8 = 0x02;
const HID_REQ_GET_REPORT: u8 = 0x01;
const HID_REQ_SET_REPORT: u8 = 0x09;

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
///
/// This should work like the usb serial trait
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
    interface_number: InterfaceNumber,
    out_endpoint: EndpointOut<'a, B>, //todo why two endpoints?
    in_endpoint: EndpointIn<'a, B>,
    report_descriptor: &'static [u8],
    string_index: StringIndex,
}

impl<B: UsbBus> HIDBootKeyboard<'_, B> {
    pub fn new<I: Into<Milliseconds>>(
        usb_alloc: &'_ UsbBusAllocator<B>,
        interval: I,
    ) -> HIDBootKeyboard<'_, B> {
        //interval in example implementation is 18ms
        let interval_ms = interval.into().integer() as u8;

        let interface_number = usb_alloc.interface();

        info!("Assigned interface {:X}", u8::from(interface_number));

        HIDBootKeyboard {
            interface_number,
            //Report size must be 8 bytes or less, so also capping the packet size
            out_endpoint: usb_alloc.interrupt(8, interval_ms),
            in_endpoint: usb_alloc.interrupt(8, interval_ms),
            report_descriptor: &descriptors::HID_BOOT_KEYBOARD_REPORT_DESCRIPTOR,
            string_index: usb_alloc.string(),
        }
    }

    fn hid_descriptor(&self) -> [u8; 7] {
        [
            HID_CLASS_SPEC_1_11[0], //HID Class spec version 1.10 (BCD)
            HID_CLASS_SPEC_1_11[1],
            HID_COUNTRY_CODE_NOT_SUPPORTED, //Country code 0 - not supported
            1,                              //Num descriptors - 1
            HIDDescriptorType::Report as u8, //Class descriptor type is report
            (self.report_descriptor.len() & 0xFF) as u8, //Descriptor length
            (self.report_descriptor.len() >> 8 & 0xFF) as u8,
        ]
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
        trace!(
            "Getting config descriptor for interface {:X}",
            u8::from(self.interface_number)
        );
        writer.interface_alt(
            self.interface_number,
            usb_device::device::DEFAULT_ALTERNATE_SETTING,
            USB_CLASS_HID,
            HIDSubclass::Boot as u8,
            HIDProtocol::Keyboard as u8,
            Some(self.string_index),
        )?;

        if self.report_descriptor.len() > u16::max_value() as usize {
            error!(
                "Report descriptor for interface {:X}, length {} too long to fit in u16",
                u8::from(self.interface_number),
                self.report_descriptor.len()
            );
            return Err(UsbError::InvalidState);
        }

        writer.write(HIDDescriptorType::Hid as u8, &self.hid_descriptor())?;

        writer.endpoint(&self.out_endpoint)?;
        writer.endpoint(&self.in_endpoint)?;

        Ok(())
    }

    fn get_string(&self, index: StringIndex, _lang_id: u16) -> Option<&str> {
        if index == self.string_index {
            Some("Keyboard")
        } else {
            None
        }
    }

    fn control_in(&mut self, transfer: ControlIn<B>) {
        let request: &control::Request = transfer.request();

        //only respond to requests for this interface
        if request.index != u8::from(self.interface_number) as u16 {
            return;
        }

        trace!(
            "interface {:X} - request type: {:?}, request: {:X}, value: {:X}",
            u8::from(self.interface_number),
            request.request_type,
            request.request,
            request.value
        );

        match (request.request_type, request.request) {
            (control::RequestType::Standard, control::Request::GET_DESCRIPTOR) => {
                let request_value = (request.value >> 8) as u8;

                if request_value == HIDDescriptorType::Report as u8 {
                    transfer
                        .accept_with_static(self.report_descriptor)
                        .unwrap_or_else(|e| {
                            error!(
                                "interface {:X} - Failed to send report descriptor - {:?}",
                                u8::from(self.interface_number),
                                e
                            )
                        });
                } else if request_value == HIDDescriptorType::Hid as u8 {
                    let hid_descriptor = self.hid_descriptor();

                    let mut buffer = [0; 9];
                    buffer[0] = buffer.len() as u8;
                    buffer[1] = HIDDescriptorType::Hid as u8;
                    (&mut buffer[2..]).copy_from_slice(&hid_descriptor);

                    transfer.accept_with(&buffer).unwrap_or_else(|e| {
                        error!(
                            "interface {:X} - Failed to send HID descriptor - {:?}",
                            u8::from(self.interface_number),
                            e
                        )
                    });
                } else {
                    debug!(
                        "interface {:X} - unsupported - request type: {:?}, request: {:X}, value: {:X}",
                        u8::from(self.interface_number),
                        request.request_type,
                        request.request,
                        request.value
                    );
                    transfer.reject().ok(); // Not supported
                }
            }
            (control::RequestType::Class, HID_REQ_GET_REPORT) => {
                debug!(
                    "interface {:X} - unsupported - request type: {:?}, request: {:X}, value: {:X}",
                    u8::from(self.interface_number),
                    request.request_type,
                    request.request,
                    request.value
                );
                transfer.reject().ok(); // Not supported
            }
            (control::RequestType::Class, HID_REQ_GET_IDLE) => {
                debug!(
                    "interface {:X} - unsupported - request type: {:?}, request: {:X}, value: {:X}",
                    u8::from(self.interface_number),
                    request.request_type,
                    request.request,
                    request.value
                );
                transfer.reject().ok(); // Not supported
            }
            _ => {
                debug!(
                    "interface {:X} - unsupported - request type: {:?}, request: {:X}, value: {:X}",
                    u8::from(self.interface_number),
                    request.request_type,
                    request.request,
                    request.value
                );
                transfer.reject().ok(); // Not supported
            }
        }
    }

    fn control_out(&mut self, transfer: ControlOut<B>) {
        let request: &control::Request = transfer.request();

        //only respond to requests for this interface
        if request.index != u8::from(self.interface_number) as u16 {
            return;
        }
        trace!(
            "interface {:X} - request type: {:?}, request: {:X}, value: {:X}",
            u8::from(self.interface_number),
            request.request_type,
            request.request,
            request.value
        );

        match (request.request_type, request.request) {
            (control::RequestType::Class, HID_REQ_SET_IDLE) => {
                debug!(
                    "interface {:X} - unsupported - request type: {:?}, request: {:X}, value: {:X}",
                    u8::from(self.interface_number),
                    request.request_type,
                    request.request,
                    request.value
                );
                transfer.accept().ok(); //Not supported
            }
            (control::RequestType::Class, HID_REQ_SET_REPORT) => {
                debug!(
                    "interface {:X} - unsupported - request type: {:?}, request: {:X}, value: {:X}",
                    u8::from(self.interface_number),
                    request.request_type,
                    request.request,
                    request.value
                );
                transfer.reject().ok(); // Not supported
            }
            _ => {
                debug!(
                    "interface {:X} - unsupported - request type: {:?}, request: {:X}, value: {:X}",
                    u8::from(self.interface_number),
                    request.request_type,
                    request.request,
                    request.value
                );
                transfer.reject().ok();
            }
        }
    }
}
