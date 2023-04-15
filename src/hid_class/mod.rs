//! Abstract Human Interface Device Class for implementing any HID compliant device

use crate::interface::{DeviceClass, UsbAllocatable};
use crate::interface::{DeviceHList, InterfaceClass};
use crate::UsbHidError;
use core::cell::RefCell;
use core::default::Default;
use core::marker::PhantomData;
use descriptor::{DescriptorType, HidProtocol};
use frunk::hlist::{HList, Selector};
use frunk::{HCons, HNil, ToMut};
use num_enum::{IntoPrimitive, TryFromPrimitive};
#[allow(clippy::wildcard_imports)]
use usb_device::class_prelude::*;
use usb_device::control::{Recipient, Request};
use usb_device::{control::RequestType, Result};

pub mod descriptor;
pub mod prelude;
#[cfg(test)]
mod test;

#[derive(Clone, Copy, Debug, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum HidRequest {
    GetReport = 0x01,
    GetIdle = 0x02,
    GetProtocol = 0x03,
    SetReport = 0x09,
    SetIdle = 0x0A,
    SetProtocol = 0x0B,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, TryFromPrimitive, IntoPrimitive)]
#[repr(u16)]
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
pub struct UsbHidClassBuilder<'a, B, InterfaceList> {
    devices: InterfaceList,
    _marker: PhantomData<&'a B>,
}

impl<'a, B> UsbHidClassBuilder<'a, B, HNil> {
    pub fn new() -> Self {
        Self {
            devices: HNil,
            _marker: PhantomData::default(),
        }
    }
}

impl<'a, B> Default for UsbHidClassBuilder<'a, B, HNil> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, B: UsbBus, Tail: HList> UsbHidClassBuilder<'a, B, Tail> {
    pub fn add<C, D>(self, config: C) -> UsbHidClassBuilder<'a, B, HCons<C, Tail>>
    where
        C: UsbAllocatable<'a, B, Allocated = D>,
        D: DeviceClass<'a>,
    {
        UsbHidClassBuilder {
            devices: self.devices.prepend(config),
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
    ) -> UsbHidClass<B, HCons<C::Allocated, Tail::Allocated>> {
        UsbHidClass {
            interfaces: RefCell::new(self.devices.allocate(usb_alloc)),
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

impl<'a, B, Devices: DeviceHList<'a>> UsbHidClass<'a, B, Devices> {
    pub fn interface<T, Index>(&mut self) -> &mut T
    where
        Devices: Selector<T, Index>,
    {
        self.interfaces.get_mut().get_mut()
    }

    pub fn interfaces(&'a mut self) -> <Devices as ToMut>::Output {
        self.interfaces.get_mut().to_mut()
    }

    /// Call every 1ms / at 1KHz
    pub fn tick(&mut self) -> core::result::Result<(), UsbHidError> {
        self.interfaces.get_mut().tick()
    }
}

impl<'a, B: UsbBus + 'a, I> UsbHidClass<'a, B, I> {
    fn get_descriptor(transfer: ControlIn<B>, interface: &mut dyn InterfaceClass<'a>) {
        let request: &Request = transfer.request();
        match DescriptorType::try_from((request.value >> 8) as u8) {
            Ok(DescriptorType::Report) => {
                match transfer.accept_with(interface.report_descriptor()) {
                    Err(e) => error!("Failed to send report descriptor - {:?}", e),
                    Ok(_) => {
                        trace!("Sent report descriptor");
                    }
                }
            }
            Ok(DescriptorType::Hid) => {
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

impl<'a, B, I> UsbClass<B> for UsbHidClass<'a, B, I>
where
    B: UsbBus + 'a,
    I: DeviceHList<'a>,
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

        let Some(interface) = u8::try_from(request.index)
                    .ok()
                    .and_then(|id| self.interfaces.get_mut().get(id)) else { return };

        trace!(
            "ctrl_out: request type: {:?}, request: {}, value: {}",
            request.request_type,
            request.request,
            request.value
        );

        match HidRequest::try_from(request.request) {
            Ok(HidRequest::SetReport) => {
                interface.set_report(transfer.data()).ok();
                transfer.accept().ok();
            }
            Ok(HidRequest::SetIdle) => {
                if request.length != 0 {
                    warn!(
                        "Expected SetIdle to have length 0, received {}",
                        request.length
                    );
                }

                interface.set_idle((request.value & 0xFF) as u8, (request.value >> 8) as u8);
                transfer.accept().ok();
            }
            Ok(HidRequest::SetProtocol) => {
                if request.length != 0 {
                    warn!(
                        "Expected SetProtocol to have length 0, received {}",
                        request.length
                    );
                }
                if let Ok(protocol) = HidProtocol::try_from((request.value & 0xFF) as u8) {
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
                if let Some(interface) = self.interfaces.get_mut().get(interface_id) {
                    if request.request == Request::GET_DESCRIPTOR {
                        info!("Get descriptor");
                        Self::get_descriptor(transfer, interface);
                    }
                }
            }

            RequestType::Class => {
                let Some(interface) = self.interfaces.get_mut().get(interface_id) else {
                    return;
                };

                match HidRequest::try_from(request.request) {
                    Ok(HidRequest::GetReport) => {
                        let mut data = [0_u8; 64];
                        if let Ok(n) = interface.get_report(&mut data) {
                            if n != transfer.request().length.into() {
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
                    Ok(HidRequest::GetIdle) => {
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
                    Ok(HidRequest::GetProtocol) => {
                        if request.length != 1 {
                            warn!(
                                "Expected GetProtocol to have length 1, received {}",
                                request.length
                            );
                        }

                        let protocol = interface.get_protocol();
                        if let Err(e) = transfer.accept_with(&[protocol.into()]) {
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
