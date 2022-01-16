//!Implements UsbHidClass representing a USB Human Interface Device

pub mod control;
pub mod descriptor;
#[cfg(test)]
mod test;

use control::HidRequest;
use descriptor::*;
use embedded_time::duration::*;
use log::{error, info, trace, warn};
use packed_struct::prelude::*;
use usb_device::class_prelude::*;
use usb_device::control::Recipient;
use usb_device::control::Request;
use usb_device::control::RequestType;
use usb_device::Result;

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
    /// When set to 0, the duration is indefinite.
    /// Should be between 0.004 to 1.020 seconds and divisible by 4
    fn idle_default(&self) -> Milliseconds {
        Milliseconds(0)
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
    global_idle: u8,
    report_idle: heapless::FnvIndexMap<u8, u8, 32>,
}

impl<B: UsbBus, C: HidConfig> UsbHidClass<'_, B, C> {
    pub fn new(usb_alloc: &'_ UsbBusAllocator<B>, hid_class: C) -> UsbHidClass<'_, B, C> {
        let interval_ms = hid_class.poll_interval().integer() as u8;
        let interface_number = usb_alloc.interface();
        let global_idle = (hid_class.idle_default().integer() / 4) as u8;

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
            global_idle,
            report_idle: heapless::FnvIndexMap::new(),
        }
    }

    fn idle_from_ms(idle: Milliseconds) -> u8 {
        (idle.integer() / 4) as u8
    }

    pub fn protocol(&self) -> HidProtocol {
        self.protocol
    }

    pub fn global_idle(&self) -> Milliseconds {
        Milliseconds((self.global_idle as u32) * 4)
    }

    pub fn report_idle(&self, report_id: u8) -> Option<Milliseconds> {
        if report_id == 0 {
            None
        } else {
            self.report_idle
                .get(&report_id)
                .map(|&i| Milliseconds((i as u32) * 4))
        }
    }

    pub fn write_report(&self, data: &[u8]) -> Result<usize> {
        self.in_endpoint.write(data)
    }

    pub fn read_report(&self, data: &mut [u8]) -> Result<usize> {
        self.out_endpoint.read(data)
    }

    fn control_in_get_descriptors(&mut self, transfer: ControlIn<B>) {
        let request: &Request = transfer.request();
        match DescriptorType::from_primitive((request.value >> 8) as u8) {
            Some(DescriptorType::Report) => {
                match transfer.accept_with_static(self.hid_class.report_descriptor()) {
                    Err(e) => error!("Failed to send report descriptor - {:?}", e),
                    Ok(_) => {
                        trace!("Sent report descriptor")
                    }
                }
            }
            Some(DescriptorType::Hid) => {
                match hid_descriptor(self.hid_class.report_descriptor().len()) {
                    Err(e) => {
                        error!("Failed to generate Hid descriptor - {:?}", e);
                        transfer.reject().ok();
                    }
                    Ok(hid_descriptor) => {
                        let mut buffer = [0; 9];
                        buffer[0] = buffer.len() as u8;
                        buffer[1] = DescriptorType::Hid as u8;
                        (&mut buffer[2..]).copy_from_slice(&hid_descriptor);
                        match transfer.accept_with(&buffer) {
                            Err(e) => {
                                error!("Failed to send Hid descriptor - {:?}", e);
                            }
                            Ok(_) => {
                                trace!("Sent hid descriptor")
                            }
                        }
                    }
                }
            }
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
        writer.write(
            DescriptorType::Hid as u8,
            &hid_descriptor(self.hid_class.report_descriptor().len())?,
        )?;

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
        let request: &Request = transfer.request();
        //only respond to requests for this interface
        if !(request.recipient == Recipient::Interface
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
            RequestType::Standard => {
                if request.request == Request::GET_DESCRIPTOR {
                    self.control_in_get_descriptors(transfer);
                }
            }

            RequestType::Class => {
                match HidRequest::from_primitive(request.request) {
                    Some(HidRequest::GetReport) => {
                        warn!("Rejected get report - unsupported");
                        // Not supported - data reports handled via interrupt endpoints
                        transfer.reject().ok();
                    }
                    Some(HidRequest::GetIdle) => {
                        if request.length != 1 {
                            warn!(
                                "Expected GetIdle to have length 1, received {:X}",
                                request.length
                            );
                        }

                        let report_id = (request.value & 0xFF) as u8;

                        let idle = if report_id == 0 {
                            self.global_idle
                        } else {
                            *self
                                .report_idle
                                .get(&report_id)
                                .unwrap_or(&self.global_idle)
                        };

                        match transfer.accept_with(&[idle]) {
                            Err(e) => error!("Failed to send idle data - {:?}", e),
                            Ok(_) => trace!("Sent idle for {:X}: {:X}", report_id, idle),
                        }
                    }
                    Some(HidRequest::GetProtocol) => {
                        if request.length != 1 {
                            warn!(
                                "Expected GetProtocol to have length 1, received {:X}",
                                request.length
                            );
                        }

                        match transfer.accept_with(&[self.protocol as u8]) {
                            Err(e) => error!("Failed to send protocol data - {:?}", e),
                            Ok(_) => trace!("Sent protocol: {:?}", self.protocol),
                        }
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
        let request: &Request = transfer.request();

        //only respond to Class requests for this interface
        if !(request.request_type == RequestType::Class
            && request.recipient == Recipient::Interface
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

        match HidRequest::from_primitive(request.request) {
            Some(HidRequest::SetIdle) => {
                if request.length != 0 {
                    warn!(
                        "Expected SetIdle to have length 0, received {:X}",
                        request.length
                    );
                }

                let idle = (request.value >> 8) as u8;
                let report_id = (request.value & 0xFF) as u8;
                if report_id == 0 {
                    self.global_idle = idle;
                    //"If the lower byte of value is zero, then the idle rate applies to all
                    //input reports generated by the device" - HID spec 7.2.4
                    self.report_idle.clear();
                    trace!("Set global idle to {:X} and cleared report idle map", idle);
                    transfer.accept().ok();
                } else {
                    match self.report_idle.insert(report_id, idle) {
                        Ok(_) => {
                            trace!("Set report idle for {:X} to {:X}", report_id, idle);

                            transfer.accept().ok();
                        }
                        Err(_) => {
                            error!(
                                "Failed to set idle for report id {:X}, idle map is full",
                                report_id
                            );
                            transfer.reject().ok();
                        }
                    }
                }
            }
            Some(HidRequest::SetReport) => {
                // Not supported - data reports handled via interrupt endpoints
                transfer.reject().ok();
            }
            Some(HidRequest::SetProtocol) => {
                if request.length != 0 {
                    warn!(
                        "Expected SetProtocol to have length 0, received {:X}",
                        request.length
                    );
                }
                if let Some(protocol) = HidProtocol::from_primitive((request.value & 0xFF) as u8) {
                    self.protocol = protocol;
                    trace!("Set protocol to {:?}", protocol);
                    transfer.accept().ok();
                } else {
                    error!(
                        "Unable to set protocol, unsupported value:{:X}",
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
        self.protocol = HidProtocol::Report;
        self.global_idle = Self::idle_from_ms(self.hid_class.idle_default());
        self.report_idle.clear();
    }
}
