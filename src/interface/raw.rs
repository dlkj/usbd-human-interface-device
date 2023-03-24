use crate::hid_class::descriptor::{
    DescriptorType, HidProtocol, InterfaceProtocol, InterfaceSubClass, COUNTRY_CODE_NOT_SUPPORTED,
    SPEC_VERSION_1_11, USB_CLASS_HID,
};
use crate::hid_class::{BuilderResult, UsbHidBuilderError, UsbPacketSize};
use crate::interface::{InterfaceClass, UsbAllocatable};
use crate::private::Sealed;
use core::marker::PhantomData;
use fugit::{ExtU32, MillisDurationU32};
use heapless::Vec;
use option_block::{Block128, Block16, Block32, Block64, Block8};
use packed_struct::PackedStruct;
use usb_device::bus::{InterfaceNumber, StringIndex, UsbBus, UsbBusAllocator};
#[allow(clippy::wildcard_imports)]
use usb_device::class_prelude::*;
use usb_device::UsbError;

pub trait ReportBuffer: Default {
    fn clear(&mut self);
    fn is_empty(&self) -> bool;
    fn len(&self) -> usize;
    fn capacity(&self) -> usize;
    #[allow(clippy::result_unit_err)]
    fn extend_from_slice(&mut self, other: &[u8]) -> Result<(), ()>;
    fn as_ref(&self) -> &[u8];
}

impl ReportBuffer for () {
    fn clear(&mut self) {}

    fn is_empty(&self) -> bool {
        true
    }

    fn len(&self) -> usize {
        0
    }

    fn capacity(&self) -> usize {
        0
    }

    fn extend_from_slice(&mut self, _other: &[u8]) -> Result<(), ()> {
        Err(())
    }

    fn as_ref(&self) -> &[u8] {
        &[]
    }
}

impl<const N: usize> ReportBuffer for Vec<u8, N> {
    fn clear(&mut self) {
        self.clear();
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn len(&self) -> usize {
        <[u8]>::len(self)
    }

    fn capacity(&self) -> usize {
        self.capacity()
    }

    fn extend_from_slice(&mut self, other: &[u8]) -> Result<(), ()> {
        self.extend_from_slice(other)
    }

    fn as_ref(&self) -> &[u8] {
        self
    }
}

pub trait InSize: Sealed {
    type Buffer;
}
pub enum InNone {}
impl Sealed for InNone {}
impl InSize for InNone {
    type Buffer = ();
}
pub enum InBytes8 {}
impl Sealed for InBytes8 {}
impl InSize for InBytes8 {
    type Buffer = Vec<u8, 8>;
}
pub enum InBytes16 {}
impl Sealed for InBytes16 {}
impl InSize for InBytes16 {
    type Buffer = Vec<u8, 16>;
}
pub enum InBytes32 {}
impl Sealed for InBytes32 {}
impl InSize for InBytes32 {
    type Buffer = Vec<u8, 32>;
}
pub enum InBytes64 {}
impl Sealed for InBytes64 {}
impl InSize for InBytes64 {
    type Buffer = Vec<u8, 64>;
}

pub trait OutSize: Sealed {
    type Buffer;
}
pub enum OutNone {}
impl Sealed for OutNone {}
impl OutSize for OutNone {
    type Buffer = ();
}
pub enum OutBytes8 {}
impl Sealed for OutBytes8 {}
impl OutSize for OutBytes8 {
    type Buffer = Vec<u8, 8>;
}
pub enum OutBytes16 {}
impl Sealed for OutBytes16 {}
impl OutSize for OutBytes16 {
    type Buffer = Vec<u8, 16>;
}
pub enum OutBytes32 {}
impl Sealed for OutBytes32 {}
impl OutSize for OutBytes32 {
    type Buffer = Vec<u8, 32>;
}
pub enum OutBytes64 {}
impl Sealed for OutBytes64 {}
impl OutSize for OutBytes64 {
    type Buffer = Vec<u8, 64>;
}

pub trait IdleStorage: Default {
    fn insert(&mut self, index: usize, val: u8) -> Option<u8>;
    fn get(&self, index: usize) -> Option<u8>;
}

pub trait ReportCount {
    type IdleStorage;
}

impl IdleStorage for () {
    fn insert(&mut self, _index: usize, _val: u8) -> Option<u8> {
        None
    }

    fn get(&self, _index: usize) -> Option<u8> {
        None
    }
}

pub enum SingleReport {}
impl ReportCount for SingleReport {
    type IdleStorage = ();
}

macro_rules! option_block_idle_storage {
    ($name: ident, $storage: ident) => {
        impl IdleStorage for $storage<u8> {
            fn insert(&mut self, index: usize, val: u8) -> Option<u8> {
                self.insert(index, val)
            }

            fn get(&self, index: usize) -> Option<u8> {
                self.get(index).cloned()
            }
        }

        pub enum $name {}
        impl ReportCount for $name {
            type IdleStorage = $storage<u8>;
        }
    };
}

option_block_idle_storage!(UpTo8Reports, Block8);
option_block_idle_storage!(UpTo16Reports, Block16);
option_block_idle_storage!(UpTo32Reports, Block32);
option_block_idle_storage!(UpTo64Reports, Block64);
option_block_idle_storage!(UpTo128Reports, Block128);

use super::{HidDescriptorBody, RawInterfaceT};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawInterfaceConfig<'a, I, O, R>
where
    I: InSize,
    I::Buffer: ReportBuffer,
    O: OutSize,
    O::Buffer: ReportBuffer,
    R: ReportCount,
    R::IdleStorage: IdleStorage,
{
    marker: PhantomData<(I, O, R)>,
    report_descriptor: &'a [u8],
    report_descriptor_length: u16,
    description: Option<&'a str>,
    protocol: InterfaceProtocol,
    idle_default: u8,
    out_endpoint: Option<EndpointConfig>,
    in_endpoint: EndpointConfig,
}

// TODO: make configurable, size depends on number of reports for given interface,
// in most cases Block8 (max 8 reports) would be enough (size 9B vs 36B for Block32)
type ReportIdleArray = Block32<u8>;

pub struct RawInterface<'a, B, I, O, R>
where
    B: UsbBus,
    I: InSize,
    I::Buffer: ReportBuffer,
    O: OutSize,
    O::Buffer: ReportBuffer,
    R: ReportCount,
    R::IdleStorage: IdleStorage,
{
    id: InterfaceNumber,
    config: RawInterfaceConfig<'a, I, O, R>,
    out_endpoint: Option<EndpointOut<'a, B>>,
    in_endpoint: EndpointIn<'a, B>,
    description_index: Option<StringIndex>,
    protocol: HidProtocol,
    report_idle: R::IdleStorage,
    global_idle: u8,
    control_in_report_buffer: I::Buffer,
    control_out_report_buffer: O::Buffer,
}

impl<'a, B: UsbBus + 'a, I, O, R> UsbAllocatable<'a, B> for RawInterfaceConfig<'a, I, O, R>
where
    B: UsbBus,
    I: InSize,
    I::Buffer: ReportBuffer,
    O: OutSize,
    O::Buffer: ReportBuffer,
    R: ReportCount,
    R::IdleStorage: IdleStorage,
{
    type Allocated = RawInterface<'a, B, I, O, R>;

    fn allocate(self, usb_alloc: &'a UsbBusAllocator<B>) -> Self::Allocated {
        RawInterface::new(usb_alloc, self)
    }
}

impl<'a, B, I, O, R> InterfaceClass<'a, B> for RawInterface<'a, B, I, O, R>
where
    B: UsbBus,
    I: InSize,
    I::Buffer: ReportBuffer,
    O: OutSize,
    O::Buffer: ReportBuffer,
    R: ReportCount,
    R::IdleStorage: IdleStorage,
{
    type I = Self;

    fn interface(&mut self) -> &mut Self {
        self
    }

    fn reset(&mut self) {
        self.protocol = HidProtocol::Report;
        self.global_idle = self.config.idle_default;
        self.clear_report_idle();
        self.control_in_report_buffer.clear();
        self.control_out_report_buffer.clear();
    }
}

impl<'a, B: UsbBus, I, O, R> RawInterface<'a, B, I, O, R>
where
    B: UsbBus,
    I: InSize,
    I::Buffer: ReportBuffer,
    O: OutSize,
    O::Buffer: ReportBuffer,
    R: ReportCount,
    R::IdleStorage: IdleStorage,
{
    pub fn new(usb_alloc: &'a UsbBusAllocator<B>, config: RawInterfaceConfig<'a, I, O, R>) -> Self {
        RawInterface {
            id: usb_alloc.interface(),
            in_endpoint: usb_alloc.interrupt(
                config.in_endpoint.max_packet_size.into(),
                config.in_endpoint.poll_interval,
            ),
            out_endpoint: config
                .out_endpoint
                .map(|c| usb_alloc.interrupt(c.max_packet_size.into(), c.poll_interval)),
            description_index: config.description.map(|_| usb_alloc.string()),
            //When initialized, all devices default to report protocol - Hid spec 7.2.6 Set_Protocol Request
            protocol: HidProtocol::Report,
            report_idle: R::IdleStorage::default(),
            global_idle: config.idle_default,
            control_in_report_buffer: I::Buffer::default(),
            control_out_report_buffer: O::Buffer::default(),
            config,
        }
    }

    fn clear_report_idle(&mut self) {
        self.report_idle = R::IdleStorage::default();
    }
    fn get_report_idle(&self, report_id: u8) -> Option<u8> {
        if u32::from(report_id) < ReportIdleArray::CAPACITY {
            self.report_idle.get(report_id.into())
        } else {
            None
        }
    }
    #[must_use]
    pub fn protocol(&self) -> HidProtocol {
        self.protocol
    }
    #[must_use]
    pub fn global_idle(&self) -> MillisDurationU32 {
        (u32::from(self.global_idle) * 4).millis()
    }
    #[must_use]
    pub fn report_idle(&self, report_id: u8) -> Option<MillisDurationU32> {
        if report_id == 0 {
            None
        } else {
            self.get_report_idle(report_id)
                .map(|i| (u32::from(i) * 4).millis())
        }
    }
    pub fn write_report(&mut self, data: &[u8]) -> usb_device::Result<usize> {
        //Try to write report to the report buffer for the config endpoint
        let control_result = if self.control_in_report_buffer.is_empty() {
            match self.control_in_report_buffer.extend_from_slice(data) {
                Ok(_) => Ok(data.len()),
                Err(_) => Err(UsbError::BufferOverflow),
            }
        } else {
            Err(UsbError::WouldBlock)
        };

        //Also try to write report to the in endpoint
        let endpoint_result = self.in_endpoint.write(data);

        match (control_result, endpoint_result) {
            //OK if either succeeded
            (_, Ok(n)) | (Ok(n), _) => Ok(n),
            //non-WouldBlock errors take preference
            (Err(e), Err(UsbError::WouldBlock)) | (_, Err(e)) => Err(e),
        }
    }
    pub fn read_report(&mut self, data: &mut [u8]) -> usb_device::Result<usize> {
        //If there is an out endpoint, try to read from it first
        let ep_result = if let Some(ep) = &self.out_endpoint {
            ep.read(data)
        } else {
            Err(UsbError::WouldBlock)
        };

        match ep_result {
            Err(UsbError::WouldBlock) => {
                //If there wasn't data available from the in endpoint
                //try the config endpoint report buffer
                let out_len = self.control_out_report_buffer.len();
                if self.control_out_report_buffer.is_empty() {
                    Err(UsbError::WouldBlock)
                } else if data.len() < out_len {
                    Err(UsbError::BufferOverflow)
                } else {
                    data[..out_len].copy_from_slice(self.control_out_report_buffer.as_ref());
                    self.control_out_report_buffer.clear();
                    Ok(out_len)
                }
            }
            _ => ep_result,
        }
    }
}
impl<'a, B: UsbBus, I, O, R> RawInterfaceT<'a, B> for RawInterface<'a, B, I, O, R>
where
    B: UsbBus,
    I: InSize,
    I::Buffer: ReportBuffer,
    O: OutSize,
    O::Buffer: ReportBuffer,
    R: ReportCount,
    R::IdleStorage: IdleStorage,
{
    fn hid_descriptor_body(&self) -> [u8; 7] {
        match (HidDescriptorBody {
            bcd_hid: SPEC_VERSION_1_11,
            country_code: COUNTRY_CODE_NOT_SUPPORTED,
            num_descriptors: 1,
            descriptor_type: DescriptorType::Report,
            descriptor_length: self.config.report_descriptor_length,
        }
        .pack())
        {
            Ok(d) => d,
            Err(_) => panic!("Failed to pack HidDescriptor"),
        }
    }

    fn report_descriptor(&self) -> &'_ [u8] {
        self.config.report_descriptor
    }

    fn id(&self) -> InterfaceNumber {
        self.id
    }
    fn write_descriptors(&self, writer: &mut DescriptorWriter) -> usb_device::Result<()> {
        writer.interface_alt(
            self.id,
            usb_device::device::DEFAULT_ALTERNATE_SETTING,
            USB_CLASS_HID,
            InterfaceSubClass::from(self.config.protocol).into(),
            self.config.protocol.into(),
            self.description_index,
        )?;

        //Hid descriptor
        writer.write(DescriptorType::Hid.into(), &self.hid_descriptor_body())?;

        //Endpoint descriptors
        writer.endpoint(&self.in_endpoint)?;
        if let Some(e) = &self.out_endpoint {
            writer.endpoint(e)?;
        }

        Ok(())
    }
    fn get_string(&self, index: StringIndex, _lang_id: u16) -> Option<&'a str> {
        self.description_index
            .filter(|&i| i == index)
            .and(self.config.description)
    }
    fn reset(&mut self) {
        self.protocol = HidProtocol::Report;
        self.global_idle = self.config.idle_default;
        self.clear_report_idle();
        self.control_in_report_buffer = I::Buffer::default();
        self.control_out_report_buffer = O::Buffer::default();
    }
    fn set_report(&mut self, data: &[u8]) -> usb_device::Result<()> {
        if self.control_out_report_buffer.is_empty() {
            if self
                .control_out_report_buffer
                .extend_from_slice(data)
                .is_ok()
            {
                trace!(
                    "Set report, {:X} bytes",
                    &self.control_out_report_buffer.len()
                );
                Ok(())
            } else {
                error!(
                    "Failed to set report, too large for buffer. Report size {:X}, expected <={:X}",
                    data.len(),
                    &self.control_out_report_buffer.capacity()
                );
                Err(UsbError::BufferOverflow)
            }
        } else {
            trace!("Failed to set report, buffer not empty");
            Err(UsbError::WouldBlock)
        }
    }

    fn get_report(&self, data: &mut [u8]) -> usb_device::Result<usize> {
        if self.control_in_report_buffer.is_empty() {
            trace!("GetReport would block, empty buffer");
            Err(UsbError::WouldBlock)
        } else if data.len() < self.control_in_report_buffer.len() {
            error!("GetReport failed, buffer too short");
            Err(UsbError::BufferOverflow)
        } else {
            data[..self.control_in_report_buffer.len()]
                .copy_from_slice(self.control_in_report_buffer.as_ref());
            Ok(self.control_in_report_buffer.len())
        }
    }

    fn get_report_ack(&mut self) -> usb_device::Result<()> {
        if self.control_in_report_buffer.is_empty() {
            error!("GetReport ACK failed, empty buffer");
            Err(UsbError::WouldBlock)
        } else {
            self.control_in_report_buffer.clear();
            Ok(())
        }
    }

    fn set_idle(&mut self, report_id: u8, value: u8) {
        if report_id == 0 {
            self.global_idle = value;
            //"If the lower byte of value is zero, then the idle rate applies to all
            //input reports generated by the device" - HID spec 7.2.4
            self.clear_report_idle();
            info!("Set global idle to {:X}", value);
        } else if u32::from(report_id) < ReportIdleArray::CAPACITY {
            self.report_idle.insert(report_id.into(), value);
            info!("Set report idle for ID{:X} to {:X}", report_id, value);
        } else {
            warn!(
                "Failed to set idle for report id {:X} - max id {:X}",
                report_id,
                ReportIdleArray::CAPACITY - 1
            );
        }
    }
    fn get_idle(&self, report_id: u8) -> u8 {
        if report_id == 0 {
            self.global_idle
        } else {
            self.get_report_idle(report_id).unwrap_or(self.global_idle)
        }
    }
    fn set_protocol(&mut self, protocol: HidProtocol) {
        self.protocol = protocol;
        info!("Set protocol to {:?}", protocol);
    }

    fn get_protocol(&self) -> HidProtocol {
        self.protocol
    }
}

// impl<'a, B, I, O, R> RawInterface<'a, B, I, O, R>
// where
//     B: UsbBus,
//     I: InSize,
//     I::Buffer: ReportBuffer,
//     O: OutSize,
//     O::Buffer: ReportBuffer,
//     R: ReportCount,
//     R::IdleStorage: IdleStorage,
// {
//     fn clear_report_idle(&mut self) {
//         self.report_idle = R::IdleStorage::default();
//     }
//     fn get_report_idle(&self, report_id: u8) -> Option<u8> {
//         if u32::from(report_id) < ReportIdleArray::CAPACITY {
//             self.report_idle.get(usize::from(report_id))
//         } else {
//             None
//         }
//     }
//     pub fn protocol(&self) -> HidProtocol {
//         self.protocol
//     }
//     pub fn global_idle(&self) -> MillisDurationU32 {
//         (u32::from(self.global_idle) * 4).millis()
//     }
//     pub fn report_idle(&self, report_id: u8) -> Option<MillisDurationU32> {
//         if report_id == 0 {
//             None
//         } else {
//             self.get_report_idle(report_id)
//                 .map(|i| (u32::from(i) * 4).millis())
//         }
//     }
//     pub fn write_report(&self, data: &[u8]) -> usb_device::Result<usize> {
//         //Try to write report to the report buffer for the config endpoint
//         let mut in_buffer = self.control_in_report_buffer.borrow_mut();
//         let control_result = if in_buffer.is_empty() {
//             match in_buffer.extend_from_slice(data) {
//                 Ok(_) => Ok(data.len()),
//                 Err(_) => Err(UsbError::BufferOverflow),
//             }
//         } else {
//             Err(UsbError::WouldBlock)
//         };

//         //Also try to write report to the in endpoint
//         let endpoint_result = self.in_endpoint.write(data);

//         match (control_result, endpoint_result) {
//             //OK if either succeeded
//             (_, Ok(n)) | (Ok(n), _) => Ok(n),
//             //non-WouldBlock errors take preference
//             (Err(e), Err(UsbError::WouldBlock)) | (_, Err(e)) => Err(e),
//         }
//     }
//     pub fn read_report(&self, data: &mut [u8]) -> usb_device::Result<usize> {
//         //If there is an out endpoint, try to read from it first
//         let ep_result = if let Some(ep) = &self.out_endpoint {
//             ep.read(data)
//         } else {
//             Err(UsbError::WouldBlock)
//         };

//         match ep_result {
//             Err(UsbError::WouldBlock) => {
//                 //If there wasn't data available from the in endpoint
//                 //try the config endpoint report buffer
//                 let mut out_buffer = self.control_out_report_buffer.borrow_mut();
//                 if out_buffer.is_empty() {
//                     Err(UsbError::WouldBlock)
//                 } else if data.len() < out_buffer.len() {
//                     Err(UsbError::BufferOverflow)
//                 } else {
//                     let n = out_buffer.len();
//                     data[..n].copy_from_slice(out_buffer.as_ref());
//                     out_buffer.clear();
//                     Ok(n)
//                 }
//             }
//             _ => ep_result,
//         }
//     }
// }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointConfig {
    pub poll_interval: u8,
    pub max_packet_size: UsbPacketSize,
}

#[must_use = "this `UsbHidInterfaceBuilder` must be assigned or consumed by `::build_interface()`"]
#[derive(Copy, Clone, Debug)]
pub struct RawInterfaceBuilder<'a, I, O, R>
where
    I: InSize,
    I::Buffer: ReportBuffer,
    O: OutSize,
    O::Buffer: ReportBuffer,
    R: ReportCount,
    R::IdleStorage: IdleStorage,
{
    config: RawInterfaceConfig<'a, I, O, R>,
}

impl<'a, I, O, R> RawInterfaceBuilder<'a, I, O, R>
where
    I: InSize,
    I::Buffer: ReportBuffer,
    O: OutSize,
    O::Buffer: ReportBuffer,
    R: ReportCount,
    R::IdleStorage: IdleStorage,
{
    pub fn new(report_descriptor: &'a [u8]) -> BuilderResult<Self> {
        Ok(RawInterfaceBuilder {
            config: RawInterfaceConfig {
                marker: PhantomData::default(),
                report_descriptor,
                report_descriptor_length: u16::try_from(report_descriptor.len())
                    .map_err(|_| UsbHidBuilderError::SliceLengthOverflow)?,
                description: None,
                protocol: InterfaceProtocol::None,
                idle_default: 0,
                out_endpoint: None,
                in_endpoint: EndpointConfig {
                    max_packet_size: UsbPacketSize::Bytes8,
                    poll_interval: 20,
                },
            },
        })
    }

    pub fn boot_device(mut self, protocol: InterfaceProtocol) -> Self {
        self.config.protocol = protocol;
        self
    }

    pub fn idle_default(mut self, duration: MillisDurationU32) -> BuilderResult<Self> {
        if duration.ticks() == 0 {
            self.config.idle_default = 0;
        } else {
            let scaled_duration = duration.to_millis() / 4;

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
        poll_interval: MillisDurationU32,
    ) -> BuilderResult<Self> {
        self.config.out_endpoint = Some(EndpointConfig {
            max_packet_size,
            poll_interval: u8::try_from(poll_interval.to_millis())
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
        poll_interval: MillisDurationU32,
    ) -> BuilderResult<Self> {
        self.config.in_endpoint = EndpointConfig {
            max_packet_size,
            poll_interval: u8::try_from(poll_interval.to_millis())
                .map_err(|_| UsbHidBuilderError::ValueOverflow)?,
        };
        Ok(self)
    }

    #[must_use]
    pub fn build(self) -> RawInterfaceConfig<'a, I, O, R> {
        self.config
    }
}
