//!Implements HID keyboard devices
mod descriptors;

use embedded_time::duration::*;
use log::{debug, error, info, trace, warn};
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
    fn write_keycodes<K>(&self, keycodes: K) -> Result<()>
    where
        K: IntoIterator<Item = u8>;

    /// Read LED status from the host system
    fn read_leds(&self) -> Result<u8>;
}

/// Implements a HID Keyboard that conforms to the Boot specification. This aims
/// to be compatible with BIOS and other reduced functionality USB hosts
///
/// This is defined in Appendix B.1 of Device Class Definition for Human
/// Interface Devices (HID) Version 1.11 -
/// https://www.usb.org/sites/default/files/hid1_11.pdf
pub struct HIDBootKeyboard<'a, B: UsbBus> {
    interface_number: InterfaceNumber,
    out_endpoint: EndpointOut<'a, B>,
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

    fn hid_descriptor(&self) -> Result<[u8; 7]> {
        if self.report_descriptor.len() > u16::max_value() as usize {
            error!(
                "Report descriptor for interface {:X}, length {} too long to fit in u16",
                u8::from(self.interface_number),
                self.report_descriptor.len()
            );
            Err(UsbError::InvalidState)
        } else {
            Ok([
                HID_CLASS_SPEC_1_11[0], //HID Class spec version 1.10 (BCD)
                HID_CLASS_SPEC_1_11[1],
                HID_COUNTRY_CODE_NOT_SUPPORTED, //Country code 0 - not supported
                1,                              //Num descriptors - 1
                HIDDescriptorType::Report as u8, //Class descriptor type is report
                (self.report_descriptor.len() & 0xFF) as u8, //Descriptor length
                (self.report_descriptor.len() >> 8 & 0xFF) as u8,
            ])
        }
    }
}

impl<B: UsbBus> HIDKeyboard for HIDBootKeyboard<'_, B> {
    fn write_keycodes<K>(&self, keycodes: K) -> core::result::Result<(), usb_device::UsbError>
    where
        K: IntoIterator<Item = u8>,
    {
        let mut data = [0; 8];

        let mut reporting_error = false;
        let mut key_report_idx = 2;

        for k in keycodes {
            if k == 0x00 {
                //ignore none keycode
            } else if (0xE0..=0xE7).contains(&k) {
                //modifiers
                data[0] |= 1 << (k - 0xE0);
            } else if !reporting_error {
                //only do other keycodes if we aren't already sending an error
                if k < 0x04 {
                    //Fill report if any keycode is an error
                    (&mut data[2..]).fill(k);
                    reporting_error = true;
                } else if key_report_idx < data.len() {
                    //Report a standard keycode
                    data[key_report_idx] = k;
                    key_report_idx += 1;
                } else {
                    //Rollover if full
                    (&mut data[2..]).fill(0x1);
                    reporting_error = true;
                }
            }
        }

        match self.in_endpoint.write(&data) {
            Ok(8) => Ok(()),
            Ok(n) => {
                warn!(
                    "interface {:X} sent {:X} bytes, expected 8 byte",
                    n,
                    u8::from(self.interface_number),
                );
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    fn read_leds(&self) -> core::result::Result<u8, usb_device::UsbError> {
        let mut data = [0; 8];
        match self.out_endpoint.read(&mut data) {
            Ok(0) => {
                error!(
                    "interface {:X} received zero length report, expected 1 byte",
                    u8::from(self.interface_number),
                );
                Err(UsbError::ParseError)
            }
            Ok(_) => Ok(data[0]),
            Err(e) => Err(e),
        }
        //todo convert to bitflags
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

        //HID descriptor
        writer.write(HIDDescriptorType::Hid as u8, &self.hid_descriptor()?)?;

        //Endpoint descriptors
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
                    match self.hid_descriptor() {
                        Err(e) => {
                            error!(
                                "interface {:X} - Failed to generate HID descriptor - {:?}",
                                u8::from(self.interface_number),
                                e
                            );
                            transfer.reject().ok();
                        }
                        Ok(hid_descriptor) => {
                            let mut buffer = [0; 9];
                            buffer[0] = buffer.len() as u8;
                            buffer[1] = HIDDescriptorType::Hid as u8;
                            (&mut buffer[2..]).copy_from_slice(&hid_descriptor);
                            transfer.accept_with(&buffer).unwrap_or_else(|e| {
                                error!(
                                    "interface {:X} - Failed to send HID descriptor - {:?}",
                                    u8::from(self.interface_number),
                                    e
                                );
                            });
                        }
                    }
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
