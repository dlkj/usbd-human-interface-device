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

    pub fn build(self) -> BuilderResult<UsbHidClass<'a, B, HCons<Interface<'a, B>, HNil>>> {
        if self.interfaces.is_empty() {
            Err(UsbHidBuilderError::NoInterfacesDefined)
        } else {
            Ok(UsbHidClass::allocate(self.usb_alloc, self.interfaces))
        }
    }
}

type BuilderResult<B> = core::result::Result<B, UsbHidBuilderError>;

trait InterfaceHList<'a, B: UsbBus + 'a> {
    fn dyn_foldl<A, F: FnMut(A, &dyn InterfaceClass<'a, B>) -> A>(&self, acc: A, folder: F) -> A;
    fn dyn_foldl_mut<A, F>(&mut self, acc: A, folder: F) -> A
    where
        F: FnMut(A, &mut dyn InterfaceClass<'a, B>) -> A;
}

impl<'a, B: UsbBus + 'a> InterfaceHList<'a, B> for HNil {
    #[inline(always)]
    fn dyn_foldl<A, F: FnMut(A, &'a dyn InterfaceClass<'a, B>) -> A>(&self, acc: A, _: F) -> A {
        acc
    }
    #[inline(always)]
    fn dyn_foldl_mut<A, F>(&mut self, acc: A, _: F) -> A
    where
        F: FnMut(A, &mut dyn InterfaceClass<'a, B>) -> A,
    {
        acc
    }
}

impl<'a, B: UsbBus + 'a, Head: InterfaceClass<'a, B>, Tail: InterfaceHList<'a, B>>
    InterfaceHList<'a, B> for HCons<Head, Tail>
{
    #[inline(always)]
    fn dyn_foldl<A, F: FnMut(A, &dyn InterfaceClass<'a, B>) -> A>(
        &self,
        acc: A,
        mut folder: F,
    ) -> A {
        let HCons { head, tail } = self;
        let acc = folder(acc, head);
        tail.dyn_foldl(acc, folder)
    }

    fn dyn_foldl_mut<A, F>(&mut self, acc: A, mut folder: F) -> A
    where
        F: FnMut(A, &mut dyn InterfaceClass<'a, B>) -> A,
    {
        let HCons { head, tail } = self;
        let acc = folder(acc, head);
        tail.dyn_foldl_mut(acc, folder)
    }
}

/// USB Human Interface Device class
pub struct UsbHidClass<'a, B: UsbBus, I> {
    interfaces: I,
    _interfaces_vec: heapless::Vec<Interface<'a, B>, MAX_INTERFACE_COUNT>,
    _interface_num_map: heapless::FnvIndexMap<u8, usize, MAX_INTERFACE_COUNT>,
}

impl<'a, B, I> UsbAllocatable<'a, B, I> for UsbHidClass<'a, B, HCons<Interface<'a, B>, HNil>>
where
    B: UsbBus,
    I: IntoIterator<Item = InterfaceConfig<'a>>,
{
    fn allocate(usb_alloc: &'a UsbBusAllocator<B>, interface_configs: I) -> Self {
        let ic = interface_configs.into_iter().next().unwrap();
        let i = Interface::allocate(usb_alloc, ic);

        UsbHidClass {
            interfaces: hlist![i],
            _interfaces_vec: Default::default(),
            _interface_num_map: Default::default(),
        }
    }
}

impl<'a, B: UsbBus, Head, Tail> UsbHidClass<'a, B, HCons<Head, Tail>> {
    pub fn interface<T, Index>(&self) -> &T
    where
        HCons<Head, Tail>: Selector<T, Index>,
    {
        self.interfaces.get::<T, Index>()
    }
}

impl<'a, B: UsbBus, I> UsbHidClass<'a, B, I> {
    pub fn get_descriptor(&self, transfer: ControlIn<B>, report_descriptor: &[u8]) {
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

impl<'a, B: UsbBus, I: InterfaceHList<'a, B>> UsbClass<B> for UsbHidClass<'a, B, I> {
    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> Result<()> {
        self.interfaces.dyn_foldl(Ok(()), |acc, i| {
            if acc.is_err() {
                acc
            } else {
                i.write_descriptors(writer)
            }
        })?;

        info!("wrote class config descriptor");
        Ok(())
    }

    fn get_string(&self, index: StringIndex, lang_id: u16) -> Option<&str> {
        self.interfaces.dyn_foldl(None, |acc: Option<&'a str>, i| {
            if acc.is_some() {
                acc
            } else {
                i.get_string(index, lang_id)
            }
        })
    }

    fn reset(&mut self) {
        info!("Reset");
        self.interfaces.dyn_foldl_mut((), |_, i| {
            i.reset();
        });
    }

    fn control_out(&mut self, transfer: ControlOut<B>) {
        let request: &Request = transfer.request();

        //only respond to Class requests for this interface
        if !(request.request_type == RequestType::Class
            && request.recipient == Recipient::Interface)
        {
            return;
        }

        let interface = u8::try_from(request.index).ok().and_then(|id| {
            self.interfaces.dyn_foldl_mut(None, |acc, i| {
                if acc.is_some() {
                    acc
                } else if u8::from(i.id()) == id {
                    Some(i)
                } else {
                    acc
                }
            })
        });

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
                let interface = self.interfaces.dyn_foldl(None, |acc, i| {
                    if acc.is_some() {
                        acc
                    } else if u8::from(i.id()) == interface_id {
                        Some(i)
                    } else {
                        acc
                    }
                });
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
                let interface = self.interfaces.dyn_foldl_mut(None, |acc, i| {
                    if acc.is_some() {
                        acc
                    } else if u8::from(i.id()) == interface_id {
                        Some(i)
                    } else {
                        acc
                    }
                });
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
