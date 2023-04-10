//! Abstract Human Interface Device Class for implementing any HID compliant device

use crate::interface::InterfaceHList;
use crate::interface::{InterfaceClass, UsbAllocatable};
use core::cell::RefCell;
use core::default::Default;
use core::marker::PhantomData;
use descriptor::{DescriptorType, HidProtocol};
use frunk::hlist::{HList, Selector};
use frunk::{HCons, HNil, ToMut};
use log::{error, info, trace, warn};
use num_enum::IntoPrimitive;
use packed_struct::prelude::*;
#[allow(clippy::wildcard_imports)]
use usb_device::class_prelude::*;
use usb_device::control::Recipient;
use usb_device::control::Request;
use usb_device::control::RequestType;
use usb_device::Result;

pub mod descriptor;
pub mod prelude;
#[cfg(test)]
mod test;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PrimitiveEnum, IntoPrimitive)]
#[repr(u8)]
pub enum HidRequest {
    GetReport = 0x01,
    GetIdle = 0x02,
    GetProtocol = 0x03,
    SetReport = 0x09,
    SetIdle = 0x0A,
    SetProtocol = 0x0B,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, PrimitiveEnum, IntoPrimitive)]
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
pub struct UsbHidClassBuilder<'a, B, InterfaceList> {
    interface_list: InterfaceList,
    _marker: PhantomData<&'a B>,
}

impl<'a, B> UsbHidClassBuilder<'a, B, HNil> {
    pub fn new() -> Self {
        Self {
            interface_list: HNil,
            _marker: PhantomData::default(),
        }
    }
}

impl<'a, B> Default for UsbHidClassBuilder<'a, B, HNil> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, B: UsbBus, I: HList> UsbHidClassBuilder<'a, B, I> {
    pub fn add_interface<Conf, Class>(
        self,
        interface_config: Conf,
    ) -> UsbHidClassBuilder<'a, B, HCons<Conf, I>>
    where
        Conf: UsbAllocatable<'a, B, Allocated = Class>,
        Class: InterfaceClass<'a, B>,
    {
        UsbHidClassBuilder {
            interface_list: self.interface_list.prepend(interface_config),
            _marker: PhantomData::default(),
        }
    }
}

impl<'a, B, C, Tail> UsbHidClassBuilder<'a, B, HCons<C, Tail>>
where
    B: UsbBus,
    Tail: UsbAllocatable<'a, B>,
    C: UsbAllocatable<'a, B>,
{
    pub fn build(
        self,
        usb_alloc: &'a UsbBusAllocator<B>,
    ) -> UsbHidClass<'a, B, HCons<C::Allocated, Tail::Allocated>> {
        UsbHidClass {
            // report_descriptors,
            // hid_descriptor_body,
            // strings,
            interfaces: RefCell::new(self.interface_list.allocate(usb_alloc)),
            _marker: PhantomData::default(),
        }
    }
}

pub type BuilderResult<B> = core::result::Result<B, UsbHidBuilderError>;

/// USB Human Interface Device class
#[derive(Debug, Eq, PartialEq)]
pub struct UsbHidClass<'a, B, I> {
    // Using a RefCell makes it simpler to implement devices as all calls to interfaces are mut
    // this could be removed, but then each usb device would need to implement a non mut borrow
    // of its `RawInterface`.
    interfaces: RefCell<I>,
    _marker: PhantomData<&'a B>,
}

impl<'a, B, InterfaceList: InterfaceHList<'a, B>> UsbHidClass<'a, B, InterfaceList> {
    pub fn interface<T, Index>(&mut self) -> &mut T
    where
        InterfaceList: Selector<T, Index>,
    {
        self.interfaces.get_mut().get_mut()
    }

    pub fn interfaces(&'a mut self) -> <InterfaceList as ToMut>::Output {
        self.interfaces.get_mut().to_mut()
    }
}

impl<'a, B: UsbBus + 'a, I> UsbHidClass<'a, B, I> {
    fn get_descriptor(transfer: ControlIn<B>, interface: &mut dyn InterfaceClass<'a, B>) {
        let request: &Request = transfer.request();
        match DescriptorType::from_primitive((request.value >> 8) as u8) {
            Some(DescriptorType::Report) => {
                match transfer.accept_with(interface.interface().report_descriptor()) {
                    Err(e) => error!("Failed to send report descriptor - {:?}", e),
                    Ok(_) => {
                        trace!("Sent report descriptor");
                    }
                }
            }
            Some(DescriptorType::Hid) => {
                let mut buffer = [0; 9];
                buffer[0] = u8::try_from(buffer.len())
                    .expect("hid descriptor length expected to be less than u8::MAX");
                buffer[1] = u8::from(DescriptorType::Hid);
                (buffer[2..]).copy_from_slice(&interface.interface().hid_descriptor_body());
                match transfer.accept_with(&buffer) {
                    Err(e) => {
                        error!("Failed to send Hid descriptor - {:?}", e);
                    }
                    Ok(_) => {
                        trace!("Sent hid descriptor");
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

impl<'a, B, I> UsbClass<B> for UsbHidClass<'a, B, I>
where
    B: UsbBus + 'a,
    I: InterfaceHList<'a, B>,
{
    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> Result<()> {
        self.interfaces.borrow_mut().write_descriptors(writer)?;
        info!("wrote class config descriptor");
        Ok(())
    }

    fn get_string(&self, index: StringIndex, lang_id: u16) -> Option<&str> {
        self.interfaces.borrow_mut().get_string(index, lang_id)
    }

    fn reset(&mut self) {
        info!("Reset");
        self.interfaces.get_mut().reset();
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
            .and_then(|id| self.interfaces.get_mut().get(id));

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
                interface.interface().set_report(transfer.data()).ok();
                transfer.accept().ok();
            }
            Some(HidRequest::SetIdle) => {
                if request.length != 0 {
                    warn!(
                        "Expected SetIdle to have length 0, received {:X}",
                        request.length
                    );
                }

                interface
                    .interface()
                    .set_idle((request.value & 0xFF) as u8, (request.value >> 8) as u8);
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
                    interface.interface().set_protocol(protocol);
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
                let interface = self.interfaces.get_mut().get(interface_id);

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
                let interface = self.interfaces.get_mut().get(interface_id);

                if interface.is_none() {
                    return;
                }
                let interface = interface.unwrap();

                match HidRequest::from_primitive(request.request) {
                    Some(HidRequest::GetReport) => {
                        let mut data = [0_u8; 64];
                        if let Ok(n) = interface.interface().get_report(&mut data) {
                            if n != transfer.request().length as usize {
                                warn!(
                                    "GetReport expected {:X} bytes, got {:X} bytes",
                                    transfer.request().length,
                                    data.len()
                                );
                            }
                            if let Err(e) = transfer.accept_with(&data[..n]) {
                                error!("Failed to send report - {:?}", e);
                            } else {
                                trace!("Sent report, {:X} bytes", n);
                                interface.interface().get_report_ack().unwrap();
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
                        let idle = interface.interface().get_idle(report_id);
                        if let Err(e) = transfer.accept_with(&[idle]) {
                            error!("Failed to send idle data - {:?}", e);
                        } else {
                            info!("Get Idle for ID{:X}: {:X}", report_id, idle);
                        }
                    }
                    Some(HidRequest::GetProtocol) => {
                        if request.length != 1 {
                            warn!(
                                "Expected GetProtocol to have length 1, received {:X}",
                                request.length
                            );
                        }

                        let protocol = interface.interface().get_protocol();
                        if let Err(e) = transfer.accept_with(&[protocol as u8]) {
                            error!("Failed to send protocol data - {:?}", e);
                        } else {
                            info!("Get protocol: {:?}", protocol);
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
