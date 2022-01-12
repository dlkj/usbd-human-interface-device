//!Implements UsbHidClass representing a USB Human Interface Device

#[cfg(test)]
mod test;

use embedded_time::duration::*;
use log::{error, info, trace, warn};
use packed_struct::prelude::*;
use usb_device::class_prelude::*;
use usb_device::Result;

const USB_CLASS_HID: u8 = 0x03;
const SPEC_VERSION_1_11: u16 = 0x0111; //1.11 in BCD
const COUNTRY_CODE_NOT_SUPPORTED: u8 = 0x0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PrimitiveEnum)]
#[repr(u8)]
pub enum Request {
    GetReport = 0x01,
    GetIdle = 0x02,
    GetProtocol = 0x03,
    SetReport = 0x09,
    SetIdle = 0x0A,
    SetProtocol = 0x0B,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum InterfaceProtocol {
    None = 0x00,
    Keyboard = 0x01,
    Mouse = 0x02,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PrimitiveEnum)]
#[repr(u8)]
pub enum DescriptorType {
    Hid = 0x21,
    Report = 0x22,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum InterfaceSubClass {
    None = 0x00,
    Boot = 0x01,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PrimitiveEnum)]
#[repr(u8)]
pub enum HidProtocol {
    Boot = 0x00,
    Report = 0x01,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PackedStruct)]
#[packed_struct(endian = "lsb", size_bytes = 7)]
struct HidDescriptorBody {
    bcd_hid: u16,
    country_code: u8,
    num_descriptors: u8,
    #[packed_field(ty = "enum", size_bytes = "1")]
    descriptor_type: DescriptorType,
    descriptor_length: u16,
}

pub trait HidConfig {
    /*todo
     * Replace with builder calling a private constructor on UsbHidClass
     * Independent config of endpoints
     * Make out endpoint optional
     * Default Idle config
     * Link boot and protocol
     */

    fn packet_size(&self) -> u8 {
        8
    }
    fn poll_interval(&self) -> Milliseconds {
        Milliseconds(10)
    }
    fn report_descriptor(&self) -> &'static [u8];
    fn interface_protocol(&self) -> InterfaceProtocol {
        InterfaceProtocol::None
    }
    fn interface_sub_class(&self) -> InterfaceSubClass {
        InterfaceSubClass::None
    }
    fn interface_name(&self) -> &str;
    fn reset(&mut self) {}
}

/// Implements the generic Hid Device
pub struct UsbHidClass<'a, B: UsbBus, C: HidConfig> {
    hid_class: C,
    interface_number: InterfaceNumber,
    out_endpoint: EndpointOut<'a, B>,
    in_endpoint: EndpointIn<'a, B>,
    string_index: StringIndex,
    protocol: HidProtocol,
}

impl<B: UsbBus, C: HidConfig> UsbHidClass<'_, B, C> {
    pub fn new(usb_alloc: &'_ UsbBusAllocator<B>, hid_class: C) -> UsbHidClass<'_, B, C> {
        let interval_ms = hid_class.poll_interval().integer() as u8;
        let interface_number = usb_alloc.interface();

        UsbHidClass {
            hid_class,
            interface_number,
            //todo make packet size configurable
            //todo make endpoints independently configurable
            //todo make endpoints optional
            out_endpoint: usb_alloc.interrupt(8, interval_ms),
            in_endpoint: usb_alloc.interrupt(8, interval_ms),
            string_index: usb_alloc.string(),
            protocol: HidProtocol::Report, //When initialized, all devices default to report protocol - Hid spec 7.2.6 Set_Protocol Request
        }
    }

    fn hid_descriptor(&self) -> Result<[u8; 7]> {
        let descriptor_len = self.hid_class.report_descriptor().len();

        if descriptor_len > u16::max_value() as usize {
            error!(
                "Report descriptor length {:X} too long to fit in u16",
                descriptor_len
            );
            Err(UsbError::InvalidState)
        } else {
            Ok(HidDescriptorBody {
                bcd_hid: SPEC_VERSION_1_11,
                country_code: COUNTRY_CODE_NOT_SUPPORTED,
                num_descriptors: 1,
                descriptor_type: DescriptorType::Report,
                descriptor_length: descriptor_len as u16,
            }
            .pack()
            .map_err(|e| {
                error!("Failed to pack HidDescriptor {}", e);
                UsbError::InvalidState
            })?)
        }
    }

    pub fn protocol(&self) -> HidProtocol {
        self.protocol
    }

    pub fn write_report(&self, data: &[u8]) -> Result<usize> {
        self.in_endpoint.write(data)
    }

    pub fn read_report(&self, data: &mut [u8]) -> Result<usize> {
        self.out_endpoint.read(data)
    }

    fn control_in_get_descriptors(&mut self, transfer: ControlIn<B>) {
        let request: &control::Request = transfer.request();
        match DescriptorType::from_primitive((request.value >> 8) as u8) {
            Some(DescriptorType::Report) => {
                transfer
                    .accept_with_static(self.hid_class.report_descriptor())
                    .unwrap_or_else(|e| error!("Failed to send report descriptor - {:?}", e));
            }
            Some(DescriptorType::Hid) => match self.hid_descriptor() {
                Err(e) => {
                    error!("Failed to generate Hid descriptor - {:?}", e);
                    transfer.reject().ok();
                }
                Ok(hid_descriptor) => {
                    let mut buffer = [0; 9];
                    buffer[0] = buffer.len() as u8;
                    buffer[1] = DescriptorType::Hid as u8;
                    (&mut buffer[2..]).copy_from_slice(&hid_descriptor);
                    transfer.accept_with(&buffer).unwrap_or_else(|e| {
                        error!("Failed to send Hid descriptor - {:?}", e);
                    });
                }
            },
            _ => {
                warn!(
                    "Unsupported descriptor type, request type:{:X?}, request:{:X}, value:{:X}",
                    request.request_type, request.request, request.value
                );
            }
        }
    }
}

impl<B: UsbBus, C: HidConfig> UsbClass<B> for UsbHidClass<'_, B, C> {
    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> Result<()> {
        writer.interface_alt(
            self.interface_number,
            usb_device::device::DEFAULT_ALTERNATE_SETTING,
            USB_CLASS_HID,
            self.hid_class.interface_sub_class() as u8,
            self.hid_class.interface_protocol() as u8,
            Some(self.string_index),
        )?;

        //Hid descriptor
        writer.write(DescriptorType::Hid as u8, &self.hid_descriptor()?)?;

        //Endpoint descriptors
        writer.endpoint(&self.out_endpoint)?;
        writer.endpoint(&self.in_endpoint)?;

        Ok(())
    }

    fn get_string(&self, index: StringIndex, _lang_id: u16) -> Option<&str> {
        if index == self.string_index {
            Some(self.hid_class.interface_name())
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
            "ctrl_in: request type: {:?}, request: {:X}, value: {:X}",
            request.request_type,
            request.request,
            request.value
        );

        match request.request_type {
            control::RequestType::Standard => {
                if request.request == control::Request::GET_DESCRIPTOR {
                    self.control_in_get_descriptors(transfer);
                }
            }

            control::RequestType::Class => {
                match Request::from_primitive(request.request) {
                    Some(Request::GetReport) => {
                        // Not supported - data reports handled via interrupt endpoints
                        transfer.reject().ok(); // Not supported
                    }
                    Some(Request::GetIdle) => {
                        if request.length != 1 {
                            warn!(
                                "Expected GetIdle to have length 1, received {:X}",
                                request.length
                            );
                        }

                        transfer.reject().ok(); // Not supported
                    }
                    Some(Request::GetProtocol) => {
                        if request.length != 1 {
                            warn!(
                                "Expected GetProtocol to have length 1, received {:X}",
                                request.length
                            );
                        }

                        let data = [self.protocol as u8];
                        transfer
                            .accept_with(&data)
                            .unwrap_or_else(|e| error!("Failed to send protocol - {:?}", e));
                    }
                    _ => {
                        error!(
                            "Unsupported control_in request type: {:?}, request: {:X}, value: {:X}",
                            request.request_type, request.request, request.value
                        );
                        transfer.reject().ok(); // Not supported
                    }
                }
            }
            _ => {}
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
            "ctrl_out: request type: {:?}, request: {:X}, value: {:X}",
            request.request_type,
            request.request,
            request.value
        );

        match Request::from_primitive(request.request) {
            Some(Request::SetIdle) => {
                if request.length != 0 {
                    warn!(
                        "Expected SetIdle to have length 0, received {:X}",
                        request.length
                    );
                }
                transfer.accept().ok(); //Not supported
            }
            Some(Request::SetReport) => {
                // Not supported - data reports handled via interrupt endpoints
                transfer.reject().ok();
            }
            Some(Request::SetProtocol) => {
                if request.length != 0 {
                    warn!(
                        "Expected SetProtocol to have length 0, received {:X}",
                        request.length
                    );
                }
                if let Some(protocol) = HidProtocol::from_primitive((request.value & 0xFF) as u8) {
                    self.protocol = protocol;
                    transfer.accept().ok();
                } else {
                    error!(
                        "Ctrl_out - Unsupported protocol value:{:X}, Unable to set protocol.",
                        request.value
                    );
                    transfer.reject().ok();
                }
            }
            _ => {
                error!(
                    "Unsupported control_out request type: {:?}, request: {:X}, value: {:X}",
                    request.request_type, request.request, request.value
                );
                transfer.reject().ok();
            }
        }
    }

    fn reset(&mut self) {
        info!("Reset");
        //Default to report protocol on reset
        self.protocol = HidProtocol::Report;
    }
}
