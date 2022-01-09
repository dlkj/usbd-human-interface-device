//!Implements generic HID type

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

pub trait HIDClass {
    fn packet_size(&self) -> u8;
    fn poll_interval(&self) -> Milliseconds;
    fn report_descriptor(&self) -> &'static [u8];
}

/// Implements the generic HID Device
pub struct HID<'a, B: UsbBus, C: HIDClass> {
    hid_class: C,
    interface_number: InterfaceNumber,
    out_endpoint: EndpointOut<'a, B>,
    in_endpoint: EndpointIn<'a, B>,
    string_index: StringIndex,
}

impl<B: UsbBus, C: HIDClass> HID<'_, B, C> {
    pub fn new(usb_alloc: &'_ UsbBusAllocator<B>, hid_class: C) -> HID<'_, B, C> {
        let interval_ms = hid_class.poll_interval().integer() as u8;
        let interface_number = usb_alloc.interface();

        info!("Assigned interface {:X}", u8::from(interface_number));

        HID {
            hid_class,
            interface_number,
            //Report size must be 8 bytes or less, so also capping the packet size
            out_endpoint: usb_alloc.interrupt(8, interval_ms),
            in_endpoint: usb_alloc.interrupt(8, interval_ms),
            string_index: usb_alloc.string(),
        }
    }

    fn hid_descriptor(&self) -> Result<[u8; 7]> {
        let descriptor_len = self.hid_class.report_descriptor().len();

        if descriptor_len > u16::max_value() as usize {
            error!(
                "Report descriptor for interface {:X}, length {} too long to fit in u16",
                u8::from(self.interface_number),
                descriptor_len
            );
            Err(UsbError::InvalidState)
        } else {
            Ok([
                HID_CLASS_SPEC_1_11[0], //HID Class spec version 1.10 (BCD)
                HID_CLASS_SPEC_1_11[1],
                HID_COUNTRY_CODE_NOT_SUPPORTED, //Country code 0 - not supported
                1,                              //Num descriptors - 1
                HIDDescriptorType::Report as u8, //Class descriptor type is report
                (descriptor_len & 0xFF) as u8,  //Descriptor length
                (descriptor_len >> 8 & 0xFF) as u8,
            ])
        }
    }

    pub fn interface_number(&self) -> InterfaceNumber {
        self.interface_number
    }

    pub fn write_report(&self, data: &[u8]) -> Result<usize> {
        self.in_endpoint.write(data)
    }

    pub fn read_report(&self, data: &mut [u8]) -> Result<usize> {
        self.out_endpoint.read(data)
    }
}

impl<B: UsbBus, C: HIDClass> UsbClass<B> for HID<'_, B, C> {
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
        if !(request.recipient == control::Recipient::Interface
            && request.index == u8::from(self.interface_number) as u16)
        {
            return;
        }

        trace!(
            "ctrl_in: interface {:X} - request type: {:?}, request: {:X}, value: {:X}",
            u8::from(self.interface_number),
            request.request_type,
            request.request,
            request.value
        );

        match (request.request_type, request.request) {
            (control::RequestType::Standard, control::Request::GET_DESCRIPTOR) => {
                let request_value = (request.value >> 8) as u8;

                if request_value == HIDDescriptorType::Report as u8 {
                    debug!(
                        "interface {:X} - get report descriptor",
                        u8::from(self.interface_number)
                    );

                    transfer
                        .accept_with_static(self.hid_class.report_descriptor())
                        .unwrap_or_else(|e| {
                            error!(
                                "interface {:X} - Failed to send report descriptor - {:?}",
                                u8::from(self.interface_number),
                                e
                            )
                        });
                } else if request_value == HIDDescriptorType::Hid as u8 {
                    debug!(
                        "interface {:X} - get hid descriptor",
                        u8::from(self.interface_number)
                    );
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
                    trace!(
                        "interface {:X} - unsupported - request type: {:?}, request: {:X}, value: {:X}",
                        u8::from(self.interface_number),
                        request.request_type,
                        request.request,
                        request.value
                    );
                }
            }
            (control::RequestType::Standard, _) => {
                // We shouldn't handle any other standard requests, leave for
                // [`UsbDevice`](crate::device::UsbDevice) to handle
            }
            (control::RequestType::Class, HID_REQ_GET_REPORT) => {
                trace!(
                    "interface {:X} - unsupported - request type: {:?}, request: {:X}, value: {:X}",
                    u8::from(self.interface_number),
                    request.request_type,
                    request.request,
                    request.value
                );

                // Not supported - data reports handled via interrupt endpoints
                transfer.reject().ok(); // Not supported
            }
            (control::RequestType::Class, HID_REQ_GET_IDLE) => {
                trace!(
                    "interface {:X} - unsupported - request type: {:?}, request: {:X}, value: {:X}",
                    u8::from(self.interface_number),
                    request.request_type,
                    request.request,
                    request.value
                );
                transfer.reject().ok(); // Not supported
            }
            _ => {
                trace!(
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

        //only respond to Class requests for this interface
        if !(request.request_type == control::RequestType::Class
            && request.recipient == control::Recipient::Interface
            && request.index == u8::from(self.interface_number) as u16)
        {
            return;
        }

        trace!(
            "ctrl_out: interface {:X} - request type: {:?}, request: {:X}, value: {:X}",
            u8::from(self.interface_number),
            request.request_type,
            request.request,
            request.value
        );

        match request.request {
            HID_REQ_SET_IDLE => {
                trace!(
                    "interface {:X} - unsupported - request type: {:?}, request: {:X}, value: {:X}",
                    u8::from(self.interface_number),
                    request.request_type,
                    request.request,
                    request.value
                );
                transfer.accept().ok(); //Not supported
            }
            HID_REQ_SET_REPORT => {
                trace!(
                    "interface {:X} - unsupported - request type: {:?}, request: {:X}, value: {:X}",
                    u8::from(self.interface_number),
                    request.request_type,
                    request.request,
                    request.value
                );

                // Not supported - data reports handled via interrupt endpoints
                transfer.reject().ok();
            }
            _ => {
                trace!(
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
