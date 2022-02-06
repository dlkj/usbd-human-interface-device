//! Abstract Human Interface Device Class for implementing any HID compliant device

use crate::hid_class::interface::{InterfaceClass, UsbAllocatable};
use core::default::Default;
use core::marker::PhantomData;
use descriptor::*;
use frunk::hlist::{HList, Selector};
use frunk::{HCons, HNil};
use interface::{Interface, InterfaceConfig, InterfaceHList};
use log::{error, info, trace, warn};
use packed_struct::prelude::*;
use usb_device::class_prelude::*;
use usb_device::control::Recipient;
use usb_device::control::Request;
use usb_device::control::RequestType;
use usb_device::Result;

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
    Bytes8 = 8,
    Bytes16 = 16,
    Bytes32 = 32,
    Bytes64 = 64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbHidBuilderError {
    ValueOverflow,
}

#[must_use = "this `UsbHidClassBuilder` must be assigned or consumed by `::build()`"]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct UsbHidClassBuilder<'a, InterfaceList> {
    interface_list: InterfaceList,
    _marker: PhantomData<&'a Self>,
}

impl<'a> UsbHidClassBuilder<'a, HNil> {
    pub fn new() -> Self {
        Self {
            interface_list: HNil,
            _marker: Default::default(),
        }
    }
}

impl<'a> Default for UsbHidClassBuilder<'a, HNil> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, I: HList> UsbHidClassBuilder<'a, I> {
    pub fn add_interface(
        self,
        interface_config: InterfaceConfig<'a>,
    ) -> UsbHidClassBuilder<'a, HCons<InterfaceConfig<'a>, I>> {
        UsbHidClassBuilder {
            interface_list: self.interface_list.prepend(interface_config),
            _marker: Default::default(),
        }
    }
}

impl<'a, Tail: HList> UsbHidClassBuilder<'a, HCons<InterfaceConfig<'a>, Tail>> {
    pub fn build<B: UsbBus>(
        self,
        usb_alloc: &'a UsbBusAllocator<B>,
    ) -> UsbHidClass<B, HCons<Interface<'a, B>, Tail::Allocated>>
    where
        Tail: UsbAllocatable<'a, B>,
    {
        UsbHidClass {
            interfaces: self.interface_list.allocate(usb_alloc),
            _marker: Default::default(),
        }
    }
}

type BuilderResult<B> = core::result::Result<B, UsbHidBuilderError>;

/// USB Human Interface Device class
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct UsbHidClass<B, I> {
    interfaces: I,
    _marker: PhantomData<B>,
}

impl<'a, B, I: InterfaceHList<'a>> UsbHidClass<B, I> {
    pub fn interface<T, Index>(&self) -> &T
    where
        I: Selector<T, Index>,
    {
        self.interfaces.get()
    }
}

impl<B: UsbBus, I> UsbHidClass<B, I> {
    fn get_descriptor(transfer: ControlIn<B>, interface: &dyn InterfaceClass<'_>) {
        let request: &Request = transfer.request();
        match DescriptorType::from_primitive((request.value >> 8) as u8) {
            Some(DescriptorType::Report) => {
                match transfer.accept_with(interface.report_descriptor()) {
                    Err(e) => error!("Failed to send report descriptor - {:?}", e),
                    Ok(_) => {
                        trace!("Sent report descriptor")
                    }
                }
            }
            Some(DescriptorType::Hid) => {
                let mut buffer = [0; 9];
                buffer[0] = buffer.len() as u8;
                buffer[1] = DescriptorType::Hid as u8;
                (&mut buffer[2..]).copy_from_slice(&interface.hid_descriptor_body());
                match transfer.accept_with(&buffer) {
                    Err(e) => {
                        error!("Failed to send Hid descriptor - {:?}", e);
                    }
                    Ok(_) => {
                        trace!("Sent hid descriptor")
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

impl<'a, B, I> UsbClass<B> for UsbHidClass<B, I>
where
    B: UsbBus,
    I: InterfaceHList<'a>,
{
    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> Result<()> {
        self.interfaces.write_descriptors(writer)?;
        info!("wrote class config descriptor");
        Ok(())
    }

    fn get_string(&self, index: StringIndex, lang_id: u16) -> Option<&str> {
        //todo - work this backward to allow strings with an 'a lifetime
        self.interfaces.get_string(index, lang_id)
    }

    fn reset(&mut self) {
        info!("Reset");
        self.interfaces.reset();
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
            .and_then(|id| self.interfaces.get_id_mut(id));

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
                interface.set_report(transfer.data()).ok();
                transfer.accept().ok();
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
                }
            }
            _ => {
                warn!(
                    "Unsupported control_out request type: {:?}, request: {:X}, value: {:X}",
                    request.request_type, request.request, request.value
                );
            }
        }
    }

    fn control_in(&mut self, transfer: ControlIn<B>) {
        let request: &Request = transfer.request();
        //only respond to requests for this interface
        if !(request.recipient == Recipient::Interface) {
            return;
        }

        let interface_id = u8::try_from(request.index).ok();

        if interface_id.is_none() {
            return;
        }
        let interface_id = interface_id.unwrap();

        trace!(
            "ctrl_in: request type: {:?}, request: {:X}, value: {:X}",
            request.request_type,
            request.request,
            request.value
        );

        match request.request_type {
            RequestType::Standard => {
                let interface = self.interfaces.get_id(interface_id);

                if interface.is_none() {
                    return;
                }
                let interface = interface.unwrap();

                if request.request == Request::GET_DESCRIPTOR {
                    info!("Get descriptor");
                    Self::get_descriptor(transfer, interface);
                }
            }

            RequestType::Class => {
                let interface = self.interfaces.get_id_mut(interface_id);

                if interface.is_none() {
                    return;
                }
                let interface = interface.unwrap();

                match HidRequest::from_primitive(request.request) {
                    Some(HidRequest::GetReport) => {
                        let mut data = [0_u8; 64];
                        if let Ok(n) = interface.get_report(&mut data) {
                            if n != transfer.request().length as usize {
                                warn!(
                                    "GetReport expected {:X} bytes, got {:X} bytes",
                                    transfer.request().length,
                                    data.len()
                                );
                            }
                            match transfer.accept_with(&data[..n]) {
                                Err(e) => error!("Failed to send report - {:?}", e),
                                Ok(()) => {
                                    trace!("Sent report, {:X} bytes", n);
                                    interface.get_report_ack().unwrap();
                                }
                            }
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
                        let idle = interface.get_idle(report_id);
                        match transfer.accept_with(&[idle]) {
                            Err(e) => error!("Failed to send idle data - {:?}", e),
                            Ok(_) => info!("Get Idle for ID{:X}: {:X}", report_id, idle),
                        }
                    }
                    Some(HidRequest::GetProtocol) => {
                        if request.length != 1 {
                            warn!(
                                "Expected GetProtocol to have length 1, received {:X}",
                                request.length
                            );
                        }

                        let protocol = interface.get_protocol();
                        match transfer.accept_with(&[protocol as u8]) {
                            Err(e) => error!("Failed to send protocol data - {:?}", e),
                            Ok(_) => info!("Get protocol: {:?}", protocol),
                        }
                    }
                    _ => {
                        warn!(
                            "Unsupported control_in request type: {:?}, request: {:X}, value: {:X}",
                            request.request_type, request.request, request.value
                        );
                    }
                }
            }
            _ => {}
        }
    }
}
