//! Abstract Human Interface Device Class for implementing any HID compliant device

use crate::interface::InterfaceHList;
use crate::interface::{InterfaceClass, UsbAllocatable};
use core::default::Default;
use core::marker::PhantomData;
use descriptor::{DescriptorType, HidProtocol};
use frunk::hlist::{HList, Selector};
use frunk::{HCons, HNil};
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

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbHidBuilderError {
    ValueOverflow,
    SliceLengthOverflow,
}

#[must_use = "this `UsbHidClassBuilder` must be assigned or consumed by `::build()`"]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct UsbHidClassBuilder<InterfaceList> {
    interface_list: InterfaceList,
}

impl UsbHidClassBuilder<HNil> {
    pub fn new() -> Self {
        Self {
            interface_list: HNil,
        }
    }
}

impl Default for UsbHidClassBuilder<HNil> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: HList> UsbHidClassBuilder<I> {
    pub fn add_interface<Conf>(self, interface_config: Conf) -> UsbHidClassBuilder<HCons<Conf, I>> {
        UsbHidClassBuilder {
            interface_list: self.interface_list.prepend(interface_config),
        }
    }
}

impl<C, Tail> UsbHidClassBuilder<HCons<C, Tail>> {
    pub fn build<'a, B>(
        self,
        usb_alloc: &'a UsbBusAllocator<B>,
    ) -> UsbHidClass<B, HCons<C::Allocated, Tail::Allocated>>
    where
        B: UsbBus,
        Tail: UsbAllocatable<'a, B>,
        C: UsbAllocatable<'a, B>,
    {
        UsbHidClass {
            interfaces: self.interface_list.allocate(usb_alloc),
            _marker: PhantomData::default(),
        }
    }
}

pub type BuilderResult<B> = core::result::Result<B, UsbHidBuilderError>;

/// USB Human Interface Device class
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct UsbHidClass<B, I> {
    interfaces: I,
    _marker: PhantomData<B>,
}

impl<'a, B, InterfaceList: InterfaceHList<'a>> UsbHidClass<B, InterfaceList> {
    pub fn interface<T, Index>(&self) -> &T
    where
        InterfaceList: Selector<T, Index>,
    {
        self.interfaces.get()
    }

    pub fn interfaces(&'a self) -> InterfaceList::Output {
        self.interfaces.to_ref()
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
                        trace!("Sent report descriptor");
                    }
                }
            }
            Some(DescriptorType::Hid) => {
                const LEN: u8 = 9;
                let mut buffer = [0; LEN as usize];
                buffer[0] = LEN;
                buffer[1] = u8::from(DescriptorType::Hid);
                (buffer[2..LEN as usize]).copy_from_slice(&interface.hid_descriptor_body());
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
                    "Unsupported descriptor type, request type:{:?}, request:{}, value:{}",
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

        let Some(interface) = u8::try_from(request.index)
                    .ok()
                    .and_then(|id| self.interfaces.get_id_mut(id)) else { return };

        trace!(
            "ctrl_out: request type: {:?}, request: {}, value: {}",
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
                        "Expected SetIdle to have length 0, received {}",
                        request.length
                    );
                }

                interface.set_idle((request.value & 0xFF) as u8, (request.value >> 8) as u8);
                transfer.accept().ok();
            }
            Some(HidRequest::SetProtocol) => {
                if request.length != 0 {
                    warn!(
                        "Expected SetProtocol to have length 0, received {}",
                        request.length
                    );
                }
                if let Some(protocol) = HidProtocol::from_primitive((request.value & 0xFF) as u8) {
                    interface.set_protocol(protocol);
                    transfer.accept().ok();
                } else {
                    error!(
                        "Unable to set protocol, unsupported value:{}",
                        request.value
                    );
                }
            }
            _ => {
                warn!(
                    "Unsupported control_out request type: {:?}, request: {}, value: {}",
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

        let Ok(interface_id) = u8::try_from(request.index) else {
            return;
        };

        trace!(
            "ctrl_in: request type: {:?}, request: {}, value: {}",
            request.request_type,
            request.request,
            request.value
        );

        match request.request_type {
            RequestType::Standard => {
                if let Some(interface) = self.interfaces.get_id(interface_id) {
                    if request.request == Request::GET_DESCRIPTOR {
                        info!("Get descriptor");
                        Self::get_descriptor(transfer, interface);
                    }
                }
            }

            RequestType::Class => {
                let Some(interface) = self.interfaces.get_id_mut(interface_id) else {
                    return;
                };

                match HidRequest::from_primitive(request.request) {
                    Some(HidRequest::GetReport) => {
                        let mut data = [0_u8; 64];
                        if let Ok(n) = interface.get_report(&mut data) {
                            if n != transfer.request().length as usize {
                                warn!(
                                    "GetReport expected {} bytes, got {} bytes",
                                    transfer.request().length,
                                    data.len()
                                );
                            }
                            if let Err(e) = transfer.accept_with(&data[..n]) {
                                error!("Failed to send report - {:?}", e);
                            } else {
                                trace!("Sent report, {} bytes", n);
                                unwrap!(interface.get_report_ack());
                            }
                        }
                    }
                    Some(HidRequest::GetIdle) => {
                        if request.length != 1 {
                            warn!(
                                "Expected GetIdle to have length 1, received {}",
                                request.length
                            );
                        }

                        let report_id = (request.value & 0xFF) as u8;
                        let idle = interface.get_idle(report_id);
                        if let Err(e) = transfer.accept_with(&[idle]) {
                            error!("Failed to send idle data - {:?}", e);
                        } else {
                            info!("Get Idle for ID{}: {}", report_id, idle);
                        }
                    }
                    Some(HidRequest::GetProtocol) => {
                        if request.length != 1 {
                            warn!(
                                "Expected GetProtocol to have length 1, received {}",
                                request.length
                            );
                        }

                        let protocol = interface.get_protocol();
                        if let Err(e) = transfer.accept_with(&[protocol as u8]) {
                            error!("Failed to send protocol data - {:?}", e);
                        } else {
                            info!("Get protocol: {:?}", protocol);
                        }
                    }
                    _ => {
                        warn!(
                            "Unsupported control_in request type: {:?}, request: {}, value: {}",
                            request.request_type, request.request, request.value
                        );
                    }
                }
            }
            _ => {}
        }
    }
}
