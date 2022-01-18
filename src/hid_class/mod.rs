//!Implements UsbHidClass representing a USB Human Interface Device

pub mod control;
pub mod descriptor;
pub mod prelude;
#[cfg(test)]
mod test;

use control::HidRequest;
use core::cell::RefCell;
use descriptor::*;
use embedded_time::duration::*;
use heapless::Vec;
use log::{error, info, trace, warn};
use packed_struct::prelude::*;
use usb_device::class_prelude::*;
use usb_device::control::Recipient;
use usb_device::control::Request;
use usb_device::control::RequestType;
use usb_device::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, PrimitiveEnum)]
#[repr(u8)]
pub enum UsbPacketSize {
    Size8 = 8,
    Size16 = 16,
    Size32 = 32,
    Size64 = 64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointConfig {
    poll_interval: u8,
    max_packet_size: UsbPacketSize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config<'a> {
    report_descriptor: &'a [u8],
    interface_description: Option<&'a str>,
    interface_protocol: InterfaceProtocol,
    idle_default: u8,
    out_endpoint: Option<EndpointConfig>,
    in_endpoint: EndpointConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbHidClassBuilderError {
    ValueOverflow,
}

#[must_use = "this `UsbHidClassBuilder` must be assigned or consumed by `::build()`"]
#[derive(Clone, Copy)]
pub struct UsbHidClassBuilder<'a, B: UsbBus> {
    usb_alloc: &'a UsbBusAllocator<B>,
    config: Config<'a>,
}

impl<'a, B: UsbBus> core::fmt::Debug for UsbHidClassBuilder<'_, B> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("UsbHidClassBuilder")
            .field("bus_alloc", &"UsbBusAllocator{...}")
            .field("config", &self.config)
            .finish()
    }
}

impl<'a, B: UsbBus> UsbHidClassBuilder<'a, B> {
    pub fn new(
        usb_alloc: &'a UsbBusAllocator<B>,
        report_descriptor: &'a [u8],
    ) -> UsbHidClassBuilder<'a, B> {
        UsbHidClassBuilder {
            usb_alloc,
            config: Config {
                report_descriptor,
                interface_description: None,
                interface_protocol: InterfaceProtocol::None,
                idle_default: 0,
                out_endpoint: None,
                in_endpoint: EndpointConfig {
                    max_packet_size: UsbPacketSize::Size8,
                    poll_interval: 20,
                },
            },
        }
    }

    pub fn boot_device(mut self, protocol: InterfaceProtocol) -> UsbHidClassBuilder<'a, B> {
        self.config.interface_protocol = protocol;
        self
    }

    pub fn idle_default<D: Into<Milliseconds>>(
        mut self,
        duration: D,
    ) -> core::result::Result<UsbHidClassBuilder<'a, B>, UsbHidClassBuilderError> {
        let d_ms = duration.into();

        if d_ms == Milliseconds(0_u32) {
            self.config.idle_default = 0;
        } else {
            let scaled_duration = d_ms.integer() / 4;

            if scaled_duration == 0 {
                //round up for 1-3ms
                self.config.idle_default = 1;
            } else {
                self.config.idle_default = u8::try_from(scaled_duration)
                    .map_err(|_| UsbHidClassBuilderError::ValueOverflow)?;
            }
        }
        Ok(self)
    }

    pub fn interface_description(mut self, s: &'a str) -> UsbHidClassBuilder<'_, B> {
        self.config.interface_description = Some(s);
        self
    }

    pub fn with_out_endpoint(
        mut self,
        max_packet_size: UsbPacketSize,
        poll_interval: Milliseconds,
    ) -> core::result::Result<UsbHidClassBuilder<'a, B>, UsbHidClassBuilderError> {
        self.config.out_endpoint = Some(EndpointConfig {
            max_packet_size,
            poll_interval: u8::try_from(poll_interval.integer())
                .map_err(|_| UsbHidClassBuilderError::ValueOverflow)?,
        });
        Ok(self)
    }

    pub fn without_out_endpoint(mut self) -> UsbHidClassBuilder<'a, B> {
        self.config.out_endpoint = None;
        self
    }

    pub fn in_endpoint(
        mut self,
        max_packet_size: UsbPacketSize,
        poll_interval: Milliseconds,
    ) -> core::result::Result<UsbHidClassBuilder<'a, B>, UsbHidClassBuilderError> {
        self.config.in_endpoint = EndpointConfig {
            max_packet_size,
            poll_interval: u8::try_from(poll_interval.integer())
                .map_err(|_| UsbHidClassBuilderError::ValueOverflow)?,
        };
        Ok(self)
    }

    pub fn build(self) -> UsbHidClass<'a, B> {
        UsbHidClass::new(self.usb_alloc, self.config)
    }
}

/// Implements the generic Hid Device
pub struct UsbHidClass<'a, B: UsbBus> {
    config: Config<'a>,
    interface_number: InterfaceNumber,
    out_endpoint: Option<EndpointOut<'a, B>>,
    in_endpoint: EndpointIn<'a, B>,
    interface_description_string_index: Option<StringIndex>,
    current_protocol: HidProtocol,
    report_idle: heapless::FnvIndexMap<u8, u8, 32>,
    global_idle: u8,
    control_in_report_buffer: RefCell<Vec<u8, 64>>,
    control_out_report_buffer: RefCell<Vec<u8, 64>>,
}

impl<'a, B: UsbBus> UsbHidClass<'a, B> {
    pub fn new(usb_alloc: &'a UsbBusAllocator<B>, config: Config<'a>) -> UsbHidClass<'a, B> {
        UsbHidClass {
            config,
            interface_number: usb_alloc.interface(),
            in_endpoint: usb_alloc.interrupt(
                config.in_endpoint.max_packet_size as u16,
                config.in_endpoint.poll_interval,
            ),
            out_endpoint: config
                .out_endpoint
                .map(|c| usb_alloc.interrupt(c.max_packet_size as u16, c.poll_interval)),
            interface_description_string_index: config
                .interface_description
                .map(|_| usb_alloc.string()),
            //When initialized, all devices default to report protocol - Hid spec 7.2.6 Set_Protocol Request
            current_protocol: HidProtocol::Report,
            report_idle: Default::default(),
            global_idle: config.idle_default,
            control_in_report_buffer: Default::default(),
            control_out_report_buffer: Default::default(),
        }
    }

    pub fn protocol(&self) -> HidProtocol {
        self.current_protocol
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
        let mut buffer = self.control_in_report_buffer.borrow_mut();
        let control_result = if buffer.is_empty() {
            match buffer.extend_from_slice(data) {
                Ok(_) => Ok(data.len()),
                Err(_) => Err(UsbError::BufferOverflow),
            }
        } else {
            Err(UsbError::WouldBlock)
        };

        let endpoint_result = self.in_endpoint.write(data);

        match (control_result, endpoint_result) {
            (_, Ok(n)) => Ok(n),
            (Ok(n), _) => Ok(n),
            (Err(UsbError::WouldBlock), Err(e)) => Err(e),
            (Err(e), Err(UsbError::WouldBlock)) => Err(e),
            (_, Err(e)) => Err(e),
        }
    }

    pub fn read_report(&self, data: &mut [u8]) -> Result<usize> {
        let ep_result = if let Some(ep) = &self.out_endpoint {
            ep.read(data)
        } else {
            //No endpoint configured
            Err(UsbError::WouldBlock)
        };

        match ep_result {
            Err(UsbError::WouldBlock) => {
                let mut buffer = self.control_out_report_buffer.borrow_mut();

                if buffer.is_empty() {
                    Err(UsbError::WouldBlock)
                } else if data.len() < buffer.len() {
                    Err(UsbError::BufferOverflow)
                } else {
                    let n = buffer.len();
                    data[..n].copy_from_slice(&buffer);
                    buffer.clear();
                    Ok(n)
                }
            }
            _ => ep_result,
        }
    }

    fn control_in_get_descriptors(&mut self, transfer: ControlIn<B>) {
        let request: &Request = transfer.request();
        match DescriptorType::from_primitive((request.value >> 8) as u8) {
            Some(DescriptorType::Report) => {
                match transfer.accept_with(self.config.report_descriptor) {
                    Err(e) => error!("Failed to send report descriptor - {:?}", e),
                    Ok(_) => {
                        trace!("Sent report descriptor")
                    }
                }
            }
            Some(DescriptorType::Hid) => {
                match hid_descriptor(self.config.report_descriptor.len()) {
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

impl<B: UsbBus> UsbClass<B> for UsbHidClass<'_, B> {
    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> Result<()> {
        writer.interface_alt(
            self.interface_number,
            usb_device::device::DEFAULT_ALTERNATE_SETTING,
            USB_CLASS_HID,
            if self.config.interface_protocol == InterfaceProtocol::None {
                InterfaceSubClass::None
            } else {
                InterfaceSubClass::Boot
            } as u8,
            self.config.interface_protocol as u8,
            self.interface_description_string_index,
        )?;

        //Hid descriptor
        writer.write(
            DescriptorType::Hid as u8,
            &hid_descriptor(self.config.report_descriptor.len())?,
        )?;

        //Endpoint descriptors
        writer.endpoint(&self.in_endpoint)?;
        if let Some(e) = &self.out_endpoint {
            writer.endpoint(e)?;
        }

        Ok(())
    }

    fn get_string(&self, index: StringIndex, _lang_id: u16) -> Option<&str> {
        self.interface_description_string_index
            .filter(|&i| i == index)
            .map(|_| self.config.interface_description)
            .flatten()
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
                        let mut buffer = self.control_in_report_buffer.borrow_mut();
                        if buffer.is_empty() {
                            trace!("Declined GetReport - empty buffer");
                            transfer.reject().ok();
                        } else {
                            match transfer.accept_with(&buffer) {
                                Err(e) => error!("Failed to send in report - {:?}", e),
                                Ok(_) => {
                                    trace!("Sent report, {:X} bytes", buffer.len())
                                }
                            }
                            buffer.clear();
                        }
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

                        match transfer.accept_with(&[self.current_protocol as u8]) {
                            Err(e) => error!("Failed to send protocol data - {:?}", e),
                            Ok(_) => trace!("Sent protocol: {:?}", self.current_protocol),
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
            Some(HidRequest::SetReport) => {
                let mut buffer = self.control_out_report_buffer.borrow_mut();
                buffer.clear();
                match buffer.extend_from_slice(transfer.data()) {
                    Err(_) => error!(
                        "Failed to receive in report. Report size {:X}, expected <={:X}",
                        transfer.data().len(),
                        buffer.capacity()
                    ),
                    Ok(_) => {
                        trace!("Received report, {:X} bytes", buffer.len())
                    }
                }
                transfer.accept().ok();
            }
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
            Some(HidRequest::SetProtocol) => {
                if request.length != 0 {
                    warn!(
                        "Expected SetProtocol to have length 0, received {:X}",
                        request.length
                    );
                }
                if let Some(protocol) = HidProtocol::from_primitive((request.value & 0xFF) as u8) {
                    self.current_protocol = protocol;
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
        self.current_protocol = HidProtocol::Report;
        self.global_idle = self.config.idle_default;
        self.report_idle.clear();
        self.control_in_report_buffer.borrow_mut().clear();
        self.control_out_report_buffer.borrow_mut().clear();
    }
}
