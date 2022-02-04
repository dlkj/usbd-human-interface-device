//! Abstract Human Interface Device Class for implementing any HID compliant device

use crate::hid_class::interface::InterfaceClass;
use crate::hid_class::interface::UsbAllocatable;
use core::default::Default;
use core::marker::PhantomData;
use descriptor::*;
use frunk::hlist::Selector;
use frunk::{hlist, HCons, HNil};
use heapless::Vec;
use interface::{Interface, InterfaceConfig};
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
pub struct UsbHidClassBuilder<'a, B: UsbBus, I> {
    usb_alloc: &'a UsbBusAllocator<B>,
    interfaces: Vec<InterfaceConfig<'a>, MAX_INTERFACE_COUNT>,
    _interface_marker: PhantomData<I>,
}

impl<'a, B: UsbBus, I> core::fmt::Debug for UsbHidClassBuilder<'_, B, I> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        //todo
        f.debug_struct("UsbHidClassBuilder")
            .field("bus_alloc", &"UsbBusAllocator{...}")
            .field("interfaces", &"Interfaces{...}")
            .finish()
    }
}

impl<'a, B: UsbBus> UsbHidClassBuilder<'a, B, HCons<Interface<'a, B>, HNil>> {
    pub fn new(usb_alloc: &'a UsbBusAllocator<B>) -> Self {
        Self {
            usb_alloc,
            interfaces: Default::default(),
            _interface_marker: Default::default(),
        }
    }

    pub fn new_interface(mut self, interface_config: InterfaceConfig<'a>) -> BuilderResult<Self> {
        self.interfaces
            .push(interface_config)
            .map_err(|_| UsbHidBuilderError::TooManyInterfaces)?;
        Ok(self)
    }

    pub fn build(self) -> BuilderResult<UsbHidClass<HCons<Interface<'a, B>, HNil>>> {
        if self.interfaces.is_empty() {
            Err(UsbHidBuilderError::NoInterfacesDefined)
        } else {
            Ok(UsbHidClass::allocate(self.usb_alloc, self.interfaces))
        }
    }
}

type BuilderResult<B> = core::result::Result<B, UsbHidBuilderError>;

pub trait InterfaceHList<'a, B: UsbBus + 'a> {
    fn get_id_mut(&mut self, id: u8) -> Option<&mut dyn InterfaceClass<'a, B>>;
    fn get_id(&self, id: u8) -> Option<&dyn InterfaceClass<'a, B>>;
    fn reset(&mut self);
    fn write_descriptors(&self, writer: &mut DescriptorWriter) -> Result<()>;
    fn get_string(&self, index: StringIndex, lang_id: u16) -> Option<&'static str>;
}

impl<'a, B: UsbBus + 'a> InterfaceHList<'a, B> for HNil {
    #[inline(always)]
    fn get_id_mut(&mut self, _: u8) -> Option<&mut dyn InterfaceClass<'a, B>> {
        None
    }
    #[inline(always)]
    fn get_id(&self, _: u8) -> Option<&dyn InterfaceClass<'a, B>> {
        None
    }
    #[inline(always)]
    fn reset(&mut self) {}
    #[inline(always)]
    fn write_descriptors(&self, _: &mut DescriptorWriter) -> Result<()> {
        Ok(())
    }
    #[inline(always)]
    fn get_string(&self, _: StringIndex, _: u16) -> Option<&'static str> {
        None
    }
}

impl<'a, B: UsbBus + 'a, Head: InterfaceClass<'a, B> + 'a, Tail: InterfaceHList<'a, B>>
    InterfaceHList<'a, B> for HCons<Head, Tail>
{
    #[inline(always)]
    fn get_id_mut(&mut self, id: u8) -> Option<&mut dyn InterfaceClass<'a, B>> {
        if id == u8::from(self.head.id()) {
            Some(&mut self.head)
        } else {
            self.tail.get_id_mut(id)
        }
    }
    #[inline(always)]
    fn get_id(&self, id: u8) -> Option<&dyn InterfaceClass<'a, B>> {
        if id == u8::from(self.head.id()) {
            Some(&self.head)
        } else {
            self.tail.get_id(id)
        }
    }
    #[inline(always)]
    fn reset(&mut self) {
        self.head.reset();
        self.tail.reset();
    }
    #[inline(always)]
    fn write_descriptors(&self, writer: &mut DescriptorWriter) -> Result<()> {
        self.head.write_descriptors(writer)?;
        self.tail.write_descriptors(writer)
    }
    #[inline(always)]
    fn get_string(&self, index: StringIndex, lang_id: u16) -> Option<&'static str> {
        let s = self.head.get_string(index, lang_id);
        if s.is_some() {
            s
        } else {
            self.tail.get_string(index, lang_id)
        }
    }
}

/// USB Human Interface Device class
pub struct UsbHidClass<I> {
    interfaces: I,
}

impl<'a, B, I> UsbAllocatable<'a, B, I> for UsbHidClass<HCons<Interface<'a, B>, HNil>>
where
    B: UsbBus,
    I: IntoIterator<Item = InterfaceConfig<'a>>,
{
    fn allocate(usb_alloc: &'a UsbBusAllocator<B>, interface_configs: I) -> Self {
        let ic = interface_configs.into_iter().next().unwrap();
        let i = Interface::allocate(usb_alloc, ic);

        UsbHidClass {
            interfaces: hlist![i],
        }
    }
}

impl<Head, Tail> UsbHidClass<HCons<Head, Tail>> {
    pub fn interface<T, Index>(&self) -> &T
    where
        HCons<Head, Tail>: Selector<T, Index>,
    {
        self.interfaces.get::<T, Index>()
    }
}

impl<I> UsbHidClass<I> {
    pub fn get_descriptor<B: UsbBus>(&self, transfer: ControlIn<B>, report_descriptor: &[u8]) {
        let request: &Request = transfer.request();
        match DescriptorType::from_primitive((request.value >> 8) as u8) {
            Some(DescriptorType::Report) => match transfer.accept_with(report_descriptor) {
                Err(e) => error!("Failed to send report descriptor - {:?}", e),
                Ok(_) => {
                    trace!("Sent report descriptor")
                }
            },
            Some(DescriptorType::Hid) => match hid_descriptor(report_descriptor.len()) {
                Err(e) => {
                    error!("Failed to generate Hid descriptor - {:?}", e);
                    //Stall, we can't make further progress without a valid descriptor
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

impl<'a, B, I> UsbClass<B> for UsbHidClass<I>
where
    B: UsbBus + 'a,
    I: InterfaceHList<'a, B> + 'a,
    Self: 'a,
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
                    self.get_descriptor(transfer, interface.report_descriptor());
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
