//! Abstract Human Interface Device Class for implementing any HID compliant device

use core::default::Default;
use embedded_time::duration::*;
use heapless::Vec;
use log::{error, info, trace, warn};
use packed_struct::prelude::*;
use usb_device::class_prelude::*;
use usb_device::control::Recipient;
use usb_device::control::Request;
use usb_device::control::RequestType;
use usb_device::Result;

use descriptor::*;

pub mod descriptor;
pub mod prelude;
#[cfg(test)]
mod test;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PrimitiveEnum)]
#[repr(u8)]
pub enum HidRequest {
    GetReport = 0x01,
    GetIdle = 0x02,
    GetProtocol = 0x03,
    SetReport = 0x09,
    SetIdle = 0x0A,
    SetProtocol = 0x0B,
}

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
pub struct InterfaceConfig<'a> {
    report_descriptor: &'a [u8],
    description: Option<&'a str>,
    protocol: InterfaceProtocol,
    idle_default: u8,
    out_endpoint: Option<EndpointConfig>,
    in_endpoint: EndpointConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbHidBuilderError {
    ValueOverflow,
    TooManyInterfaces,
    NoInterfacesDefined,
}

const MAX_INTERFACES: usize = 8;

#[must_use = "this `UsbHidClassBuilder` must be assigned or consumed by `::build()`"]
#[derive(Clone)]
pub struct UsbHidClassBuilder<'a, B: UsbBus> {
    usb_alloc: &'a UsbBusAllocator<B>,
    interfaces: Vec<InterfaceConfig<'a>, MAX_INTERFACES>,
}

impl<'a, B: UsbBus> core::fmt::Debug for UsbHidClassBuilder<'_, B> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("UsbHidClassBuilder")
            .field("bus_alloc", &"UsbBusAllocator{...}")
            .field("interfaces", &self.interfaces)
            .finish()
    }
}

impl<'a, B: UsbBus> UsbHidClassBuilder<'a, B> {
    pub fn new(usb_alloc: &'a UsbBusAllocator<B>) -> Self {
        Self {
            usb_alloc,
            interfaces: Default::default(),
        }
    }

    pub fn new_interface(self, report_descriptor: &'a [u8]) -> UsbHidInterfaceBuilder<'a, B> {
        UsbHidInterfaceBuilder::new(self, report_descriptor)
    }

    pub fn build(self) -> BuilderResult<UsbHidClass<'a, B>> {
        if self.interfaces.is_empty() {
            Err(UsbHidBuilderError::NoInterfacesDefined)
        } else {
            Ok(UsbHidClass::new(self.usb_alloc, self.interfaces))
        }
    }
}

#[must_use = "this `UsbHidInterfaceBuilder` must be assigned or consumed by `::build_interface()`"]
#[derive(Clone, Debug)]
pub struct UsbHidInterfaceBuilder<'a, B: UsbBus> {
    class_builder: UsbHidClassBuilder<'a, B>,
    config: InterfaceConfig<'a>,
}

type BuilderResult<B> = core::result::Result<B, UsbHidBuilderError>;

impl<'a, B: UsbBus> UsbHidInterfaceBuilder<'a, B> {
    fn new(class_builder: UsbHidClassBuilder<'a, B>, report_descriptor: &'a [u8]) -> Self {
        UsbHidInterfaceBuilder {
            class_builder,
            config: InterfaceConfig {
                report_descriptor,
                description: None,
                protocol: InterfaceProtocol::None,
                idle_default: 0,
                out_endpoint: None,
                in_endpoint: EndpointConfig {
                    max_packet_size: UsbPacketSize::Size8,
                    poll_interval: 20,
                },
            },
        }
    }

    pub fn boot_device(mut self, protocol: InterfaceProtocol) -> Self {
        self.config.protocol = protocol;
        self
    }

    pub fn idle_default<D: Into<Milliseconds>>(mut self, duration: D) -> BuilderResult<Self> {
        let d_ms = duration.into();

        if d_ms == Milliseconds(0_u32) {
            self.config.idle_default = 0;
        } else {
            let scaled_duration = d_ms.integer() / 4;

            if scaled_duration == 0 {
                //round up for 1-3ms
                self.config.idle_default = 1;
            } else {
                self.config.idle_default =
                    u8::try_from(scaled_duration).map_err(|_| UsbHidBuilderError::ValueOverflow)?;
            }
        }
        Ok(self)
    }

    pub fn description(mut self, s: &'a str) -> Self {
        self.config.description = Some(s);
        self
    }

    pub fn with_out_endpoint(
        mut self,
        max_packet_size: UsbPacketSize,
        poll_interval: Milliseconds,
    ) -> BuilderResult<Self> {
        self.config.out_endpoint = Some(EndpointConfig {
            max_packet_size,
            poll_interval: u8::try_from(poll_interval.integer())
                .map_err(|_| UsbHidBuilderError::ValueOverflow)?,
        });
        Ok(self)
    }

    pub fn without_out_endpoint(mut self) -> Self {
        self.config.out_endpoint = None;
        self
    }

    pub fn in_endpoint(
        mut self,
        max_packet_size: UsbPacketSize,
        poll_interval: Milliseconds,
    ) -> BuilderResult<Self> {
        self.config.in_endpoint = EndpointConfig {
            max_packet_size,
            poll_interval: u8::try_from(poll_interval.integer())
                .map_err(|_| UsbHidBuilderError::ValueOverflow)?,
        };
        Ok(self)
    }

    pub fn build_interface(mut self) -> BuilderResult<UsbHidClassBuilder<'a, B>> {
        self.class_builder
            .interfaces
            .push(self.config)
            .map_err(|_| UsbHidBuilderError::TooManyInterfaces)?;
        Ok(self.class_builder)
    }
}

pub struct Interface<'a, B: UsbBus> {
    id: InterfaceNumber,
    config: InterfaceConfig<'a>,
    out_endpoint: Option<EndpointOut<'a, B>>,
    in_endpoint: EndpointIn<'a, B>,
    description_index: Option<StringIndex>,
    protocol: HidProtocol,
    report_idle: heapless::FnvIndexMap<u8, u8, 32>,
    global_idle: u8,
    control_in_report_buffer: Vec<u8, 64>,
    control_out_report_buffer: Vec<u8, 64>,
}

impl<'a, B: UsbBus> Interface<'a, B> {
    fn new(config: InterfaceConfig<'a>, usb_alloc: &'a UsbBusAllocator<B>) -> Self {
        Interface {
            config,
            id: usb_alloc.interface(),
            in_endpoint: usb_alloc.interrupt(
                config.in_endpoint.max_packet_size as u16,
                config.in_endpoint.poll_interval,
            ),
            out_endpoint: config
                .out_endpoint
                .map(|c| usb_alloc.interrupt(c.max_packet_size as u16, c.poll_interval)),
            description_index: config.description.map(|_| usb_alloc.string()),
            //When initialized, all devices default to report protocol - Hid spec 7.2.6 Set_Protocol Request
            protocol: HidProtocol::Report,
            report_idle: Default::default(),
            global_idle: config.idle_default,
            control_in_report_buffer: Default::default(),
            control_out_report_buffer: Default::default(),
        }
    }

    pub fn protocol(&self) -> HidProtocol {
        self.protocol
    }

    pub fn id(&self) -> InterfaceNumber {
        self.id
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

    pub fn write_report(&mut self, data: &[u8]) -> Result<usize> {
        let control_result = if self.control_in_report_buffer.is_empty() {
            match self.control_in_report_buffer.extend_from_slice(data) {
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

    pub fn read_report(&mut self, data: &mut [u8]) -> Result<usize> {
        let ep_result = if let Some(ep) = &self.out_endpoint {
            ep.read(data)
        } else {
            //No endpoint configured
            Err(UsbError::WouldBlock)
        };

        match ep_result {
            Err(UsbError::WouldBlock) => {
                if self.control_out_report_buffer.is_empty() {
                    Err(UsbError::WouldBlock)
                } else if data.len() < self.control_out_report_buffer.len() {
                    Err(UsbError::BufferOverflow)
                } else {
                    let n = self.control_out_report_buffer.len();
                    data[..n].copy_from_slice(&self.control_out_report_buffer);
                    self.control_out_report_buffer.clear();
                    Ok(n)
                }
            }
            _ => ep_result,
        }
    }

    fn get_descriptor(&mut self, transfer: ControlIn<B>) {
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

    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> Result<()> {
        writer.interface_alt(
            self.id,
            usb_device::device::DEFAULT_ALTERNATE_SETTING,
            USB_CLASS_HID,
            InterfaceSubClass::from(self.config.protocol) as u8,
            self.config.protocol as u8,
            self.description_index,
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
        self.description_index
            .filter(|&i| i == index)
            .map(|_| self.config.description)
            .flatten()
    }

    fn reset(&mut self) {
        self.protocol = HidProtocol::Report;
        self.global_idle = self.config.idle_default;
        self.report_idle.clear();
        self.control_in_report_buffer.clear();
        self.control_out_report_buffer.clear();
    }

    fn set_report(&mut self, data: &[u8]) {
        self.control_out_report_buffer.clear();
        match self.control_out_report_buffer.extend_from_slice(data) {
            Err(_) => {
                error!(
                    "Failed to set report, too large for buffer. Report size {:X}, expected <={:X}",
                    data.len(),
                    &self.control_out_report_buffer.capacity()
                );
            }
            Ok(_) => {
                trace!(
                    "Received report, {:X} bytes",
                    &self.control_out_report_buffer.len()
                );
            }
        }
    }

    fn get_report(&mut self, transfer: ControlIn<B>) {
        if self.control_in_report_buffer.is_empty() {
            trace!("NAK GetReport - empty buffer");

            transfer.reject().ok(); //TODO - read docs, not sure this is correct
                                    //TODO - Should transfer be abstracted away?
        } else {
            if self.control_in_report_buffer.len() != transfer.request().length as usize {
                warn!(
                    "GetReport expected {:X} bytes, sent {:X}",
                    transfer.request().length,
                    self.control_in_report_buffer.len()
                );
            }
            match transfer.accept_with(&self.control_in_report_buffer) {
                Err(e) => error!("Failed to send in report - {:?}", e),
                Ok(_) => {
                    trace!(
                        "Sent report, {:X} bytes",
                        self.control_in_report_buffer.len()
                    )
                }
            }
        }
        self.control_in_report_buffer.clear();
    }

    fn set_idle(&mut self, report_id: u8, value: u8) {
        if report_id == 0 {
            self.global_idle = value;
            //"If the lower byte of value is zero, then the idle rate applies to all
            //input reports generated by the device" - HID spec 7.2.4
            self.report_idle.clear();
            trace!("Set global idle to {:X} and cleared report idle map", value);
        } else {
            match self.report_idle.insert(report_id, value) {
                Ok(_) => {
                    trace!("Set report idle for {:X} to {:X}", report_id, value);
                }
                Err(_) => {
                    warn!(
                        "Failed to set idle for report id {:X}, idle map is full",
                        report_id
                    );
                }
            }
        }
    }

    fn get_idle(&mut self, report_id: u8) -> u8 {
        if report_id == 0 {
            self.global_idle
        } else {
            *self
                .report_idle
                .get(&report_id)
                .unwrap_or(&self.global_idle)
        }
    }

    fn set_protocol(&mut self, protocol: HidProtocol) {
        self.protocol = protocol;
        trace!("Set protocol to {:?}", protocol);
    }
}

/// Implements the generic Hid Device
pub struct UsbHidClass<'a, B: UsbBus> {
    interfaces: heapless::Vec<Interface<'a, B>, MAX_INTERFACES>,
    interface_num_map: heapless::FnvIndexMap<u8, usize, MAX_INTERFACES>,
}

impl<'a, B: UsbBus> UsbHidClass<'a, B> {
    pub fn new<I: IntoIterator<Item = InterfaceConfig<'a>>>(
        usb_alloc: &'a UsbBusAllocator<B>,
        interface_configs: I,
    ) -> Self {
        let interfaces: heapless::Vec<Interface<'a, B>, MAX_INTERFACES> = interface_configs
            .into_iter()
            .map(|inter| Interface::new(inter, usb_alloc))
            .collect();

        let interface_num_map = interfaces
            .iter()
            .enumerate()
            .map(|(i, inter)| (u8::from(inter.id()), i))
            .collect();

        UsbHidClass {
            interfaces,
            interface_num_map,
        }
    }

    pub fn get_interface(&self, index: usize) -> Option<&Interface<'a, B>> {
        self.interfaces.get(index)
    }

    pub fn get_interface_mut(&mut self, index: usize) -> Option<&mut Interface<'a, B>> {
        self.interfaces.get_mut(index)
    }
}

impl<B: UsbBus> UsbClass<B> for UsbHidClass<'_, B> {
    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> Result<()> {
        let base_protocol = self
            .interfaces
            .iter()
            .map(|i| i.config.protocol)
            .reduce(|a, b| a.min(b))
            .unwrap_or(InterfaceProtocol::None);

        writer.iad(
            self.interfaces[0].id,
            self.interfaces.len() as u8,
            USB_CLASS_HID,
            InterfaceSubClass::from(base_protocol) as u8,
            base_protocol as u8,
        )?;

        for i in &self.interfaces {
            i.get_configuration_descriptors(writer)?;
        }

        Ok(())
    }

    fn get_string(&self, index: StringIndex, lang_id: u16) -> Option<&str> {
        for i in &self.interfaces {
            if let Some(s) = i.get_string(index, lang_id) {
                return Some(s);
            }
        }
        None
    }

    fn reset(&mut self) {
        info!("Reset");
        for i in &mut self.interfaces {
            i.reset();
        }
    }

    fn control_out(&mut self, transfer: ControlOut<B>) {
        let request: &Request = transfer.request();

        //only respond to Class requests for this interface
        if !(request.request_type == RequestType::Class
            && request.recipient == Recipient::Interface)
        {
            return;
        }

        let interface = u8::try_from(request.index)
            .ok()
            .and_then(|i| self.interface_num_map.get(&i))
            .map(|&i| &mut self.interfaces[i]);

        if interface.is_none() {
            return;
        }

        let interface = interface.unwrap();

        trace!(
            "ctrl_out: request type: {:?}, request: {:X}, value: {:X}",
            request.request_type,
            request.request,
            request.value
        );

        match HidRequest::from_primitive(request.request) {
            Some(HidRequest::SetReport) => {
                interface.set_report(transfer.data());
            }
            Some(HidRequest::SetIdle) => {
                if request.length != 0 {
                    warn!(
                        "Expected SetIdle to have length 0, received {:X}",
                        request.length
                    );
                }

                interface.set_idle((request.value & 0xFF) as u8, (request.value >> 8) as u8);
                transfer.accept().ok();
            }
            Some(HidRequest::SetProtocol) => {
                if request.length != 0 {
                    warn!(
                        "Expected SetProtocol to have length 0, received {:X}",
                        request.length
                    );
                }
                if let Some(protocol) = HidProtocol::from_primitive((request.value & 0xFF) as u8) {
                    interface.set_protocol(protocol);
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

    fn control_in(&mut self, transfer: ControlIn<B>) {
        let request: &Request = transfer.request();
        //only respond to requests for this interface
        if !(request.recipient == Recipient::Interface) {
            return;
        }

        let interface = u8::try_from(request.index)
            .ok()
            .and_then(|i| self.interface_num_map.get(&i))
            .map(|&i| &mut self.interfaces[i]);

        if interface.is_none() {
            return;
        }
        let interface = interface.unwrap();

        trace!(
            "ctrl_in: request type: {:?}, request: {:X}, value: {:X}",
            request.request_type,
            request.request,
            request.value
        );

        match request.request_type {
            RequestType::Standard => {
                if request.request == Request::GET_DESCRIPTOR {
                    interface.get_descriptor(transfer);
                }
            }

            RequestType::Class => {
                match HidRequest::from_primitive(request.request) {
                    Some(HidRequest::GetReport) => interface.get_report(transfer),
                    Some(HidRequest::GetIdle) => {
                        if request.length != 1 {
                            warn!(
                                "Expected GetIdle to have length 1, received {:X}",
                                request.length
                            );
                        }

                        let report_id = (request.value & 0xFF) as u8;
                        let idle = interface.get_idle(report_id);
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

                        let protocol = interface.protocol;
                        match transfer.accept_with(&[protocol as u8]) {
                            Err(e) => error!("Failed to send protocol data - {:?}", e),
                            Ok(_) => trace!("Sent protocol: {:?}", protocol),
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
}
