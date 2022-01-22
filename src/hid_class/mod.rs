//! Abstract Human Interface Device Class for implementing any HID compliant device

use core::default::Default;
use heapless::Vec;
use log::{error, info, trace, warn};
use packed_struct::prelude::*;
use usb_device::class_prelude::*;
use usb_device::control::Recipient;
use usb_device::control::Request;
use usb_device::control::RequestType;
use usb_device::Result;

use descriptor::*;
use interface::{Interface, InterfaceConfig, UsbHidInterfaceBuilder};

pub mod descriptor;
pub mod interface;
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
pub enum UsbHidBuilderError {
    ValueOverflow,
    TooManyInterfaces,
    NoInterfacesDefined,
}

const MAX_INTERFACE_COUNT: usize = 8;

#[must_use = "this `UsbHidClassBuilder` must be assigned or consumed by `::build()`"]
#[derive(Clone)]
pub struct UsbHidClassBuilder<'a, B: UsbBus> {
    usb_alloc: &'a UsbBusAllocator<B>,
    interfaces: Vec<InterfaceConfig<'a>, MAX_INTERFACE_COUNT>,
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

type BuilderResult<B> = core::result::Result<B, UsbHidBuilderError>;

/// Implements the generic Hid Device
pub struct UsbHidClass<'a, B: UsbBus> {
    interfaces: heapless::Vec<Interface<'a, B>, MAX_INTERFACE_COUNT>,
    interface_num_map: heapless::FnvIndexMap<u8, usize, MAX_INTERFACE_COUNT>,
}

impl<'a, B: UsbBus> UsbHidClass<'a, B> {
    pub fn new<I: IntoIterator<Item = InterfaceConfig<'a>>>(
        usb_alloc: &'a UsbBusAllocator<B>,
        interface_configs: I,
    ) -> Self {
        let interfaces: heapless::Vec<Interface<'a, B>, MAX_INTERFACE_COUNT> = interface_configs
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
            .map(|i| i.config().protocol)
            .reduce(|a, b| a.min(b))
            .unwrap_or(InterfaceProtocol::None);

        writer.iad(
            self.interfaces[0].id(),
            self.interfaces.len() as u8,
            USB_CLASS_HID,
            InterfaceSubClass::from(base_protocol) as u8,
            base_protocol as u8,
        )?;

        for i in &self.interfaces {
            i.write_descriptors(writer)?;
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

                        let protocol = interface.protocol();
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
