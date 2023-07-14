//! Human Interface Device Interfaces
use crate::descriptor::{
    DescriptorType, HidProtocol, InterfaceProtocol, InterfaceSubClass, COUNTRY_CODE_NOT_SUPPORTED,
    SPEC_VERSION_1_11, USB_CLASS_HID,
};
use crate::device::DeviceClass;
use crate::private::Sealed;
use crate::usb_class::{BuilderResult, UsbHidBuilderError};
use crate::UsbHidError;
use core::marker::PhantomData;
use frunk::{HCons, HNil};
use fugit::{ExtU32, MillisDurationU32};
use heapless::Vec;
use option_block::{Block128, Block16, Block32, Block64, Block8};
use packed_struct::prelude::*;
use packed_struct::PackedStruct;
use usb_device::bus::{StringIndex, UsbBus, UsbBusAllocator};
#[allow(clippy::wildcard_imports)]
use usb_device::class_prelude::*;
use usb_device::class_prelude::{DescriptorWriter, InterfaceNumber};
use usb_device::UsbError;

#[derive(Debug, PackedStruct)]
#[packed_struct(endian = "lsb", size_bytes = 7)]
struct HidDescriptorBody {
    bcd_hid: u16,
    country_code: u8,
    num_descriptors: u8,
    #[packed_field(ty = "enum", size_bytes = "1")]
    descriptor_type: DescriptorType,
    descriptor_length: u16,
}

pub trait UsbAllocatable<'a, B: UsbBus> {
    type Allocated;
    fn allocate(self, usb_alloc: &'a UsbBusAllocator<B>) -> Self::Allocated;
}

impl<'a, B: UsbBus + 'a> UsbAllocatable<'a, B> for HNil {
    type Allocated = Self;

    fn allocate(self, _: &'a UsbBusAllocator<B>) -> Self::Allocated {
        self
    }
}

impl<'a, B, C, Tail> UsbAllocatable<'a, B> for HCons<C, Tail>
where
    B: UsbBus + 'a,
    C: UsbAllocatable<'a, B>,
    Tail: UsbAllocatable<'a, B>,
{
    type Allocated = HCons<C::Allocated, Tail::Allocated>;

    fn allocate(self, usb_alloc: &'a UsbBusAllocator<B>) -> Self::Allocated {
        HCons {
            head: self.head.allocate(usb_alloc),
            tail: self.tail.allocate(usb_alloc),
        }
    }
}

pub trait InterfaceClass<'a> {
    fn hid_descriptor_body(&self) -> [u8; 7];
    fn report_descriptor(&self) -> &'_ [u8];
    fn id(&self) -> InterfaceNumber;
    fn write_descriptors(&self, writer: &mut DescriptorWriter) -> usb_device::Result<()>;
    fn get_string(&self, index: StringIndex, _lang_id: u16) -> Option<&'a str>;
    fn reset(&mut self);
    fn set_report(&mut self, data: &[u8]) -> usb_device::Result<()>;
    fn get_report(&self, data: &mut [u8]) -> usb_device::Result<usize>;
    fn get_report_ack(&mut self) -> usb_device::Result<()>;
    fn set_idle(&mut self, report_id: u8, value: u8);
    fn get_idle(&self, report_id: u8) -> u8;
    fn set_protocol(&mut self, protocol: HidProtocol);
    fn get_protocol(&self) -> HidProtocol;
}

pub trait ReportBuffer: Default {
    const CAPACITY: u16;
    fn clear(&mut self);
    fn is_empty(&self) -> bool;
    fn len(&self) -> usize;
    #[allow(clippy::result_unit_err)]
    fn extend_from_slice(&mut self, other: &[u8]) -> Result<(), ()>;
    fn as_ref(&self) -> &[u8];
}

impl ReportBuffer for () {
    const CAPACITY: u16 = 0;

    fn clear(&mut self) {}

    fn is_empty(&self) -> bool {
        true
    }

    fn len(&self) -> usize {
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
    // N must be < u16::MAX
    // not currently enforceable in stable
    #[allow(clippy::cast_possible_truncation)]
    const CAPACITY: u16 = N as u16;

    fn clear(&mut self) {
        self.clear();
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn len(&self) -> usize {
        <[u8]>::len(self)
    }

    fn extend_from_slice(&mut self, other: &[u8]) -> Result<(), ()> {
        self.extend_from_slice(other)
    }

    fn as_ref(&self) -> &[u8] {
        self
    }
}

pub trait InSize: Sealed {
    type Buffer: ReportBuffer;
}
pub enum InNone {}
impl Sealed for InNone {}
impl InSize for InNone {
    type Buffer = ();
}

macro_rules! vec_in_bytes {
    ($name: ident, $capacity: literal) => {
        pub enum $name {}
        impl Sealed for $name {}
        impl InSize for $name {
            type Buffer = Vec<u8, $capacity>;
        }
    };
}

vec_in_bytes!(InBytes8, 8);
vec_in_bytes!(InBytes16, 16);
vec_in_bytes!(InBytes32, 32);
vec_in_bytes!(InBytes64, 64);

pub trait OutSize: Sealed {
    type Buffer: ReportBuffer;
}
pub enum OutNone {}
impl Sealed for OutNone {}
impl OutSize for OutNone {
    type Buffer = ();
}

macro_rules! vec_out_bytes {
    ($name: ident, $capacity: literal) => {
        pub enum $name {}
        impl Sealed for $name {}
        impl OutSize for $name {
            type Buffer = Vec<u8, $capacity>;
        }
    };
}

vec_out_bytes!(OutBytes8, 8);
vec_out_bytes!(OutBytes16, 16);
vec_out_bytes!(OutBytes32, 32);
vec_out_bytes!(OutBytes64, 64);

pub trait IdleStorage: Default {
    const CAPACITY: u32;
    fn insert(&mut self, index: usize, val: u8) -> Option<u8>;
    fn get(&self, index: usize) -> Option<u8>;
}

pub trait ReportCount: Sealed {
    type IdleStorage: IdleStorage;
}

impl IdleStorage for () {
    const CAPACITY: u32 = 0;

    fn insert(&mut self, _index: usize, _val: u8) -> Option<u8> {
        None
    }

    fn get(&self, _index: usize) -> Option<u8> {
        None
    }
}

pub enum ReportSingle {}
impl Sealed for ReportSingle {}
impl ReportCount for ReportSingle {
    type IdleStorage = ();
}

macro_rules! option_block_idle_storage {
    ($name: ident, $storage: ident) => {
        impl IdleStorage for $storage<u8> {
            const CAPACITY: u32 = $storage::<u8>::CAPACITY;

            fn insert(&mut self, index: usize, val: u8) -> Option<u8> {
                self.insert(index, val)
            }

            fn get(&self, index: usize) -> Option<u8> {
                self.get(index).cloned()
            }
        }

        pub enum $name {}
        impl Sealed for $name {}
        impl ReportCount for $name {
            type IdleStorage = $storage<u8>;
        }
    };
}

option_block_idle_storage!(Reports8, Block8);
option_block_idle_storage!(Reports16, Block16);
option_block_idle_storage!(Reports32, Block32);
option_block_idle_storage!(Reports64, Block64);
option_block_idle_storage!(Reports128, Block128);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterfaceConfig<'a, I, O, R>
where
    I: InSize,
    O: OutSize,
    R: ReportCount,
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

pub struct Interface<'a, B, I, O, R>
where
    B: UsbBus,
    I: InSize,
    O: OutSize,
    R: ReportCount,
{
    id: InterfaceNumber,
    config: InterfaceConfig<'a, I, O, R>,
    out_endpoint: Option<EndpointOut<'a, B>>,
    in_endpoint: EndpointIn<'a, B>,
    description_index: Option<StringIndex>,
    protocol: HidProtocol,
    report_idle: R::IdleStorage,
    global_idle: u8,
    control_in_report_buffer: I::Buffer,
    control_out_report_buffer: O::Buffer,
}

impl<'a, B: UsbBus + 'a, I, O, R> UsbAllocatable<'a, B> for InterfaceConfig<'a, I, O, R>
where
    B: UsbBus,
    I: InSize,
    O: OutSize,
    R: ReportCount,
{
    type Allocated = Interface<'a, B, I, O, R>;

    fn allocate(self, usb_alloc: &'a UsbBusAllocator<B>) -> Self::Allocated {
        Interface::new(usb_alloc, self)
    }
}

impl<'a, B, I, O, R> DeviceClass<'a> for Interface<'a, B, I, O, R>
where
    B: UsbBus,
    I: InSize,
    O: OutSize,
    R: ReportCount,
{
    type I = Self;

    fn interface(&mut self) -> &mut Self {
        self
    }

    fn reset(&mut self) {
        <Self as InterfaceClass<'a>>::reset(self);
    }

    fn tick(&mut self) -> Result<(), crate::UsbHidError> {
        Ok(())
    }
}

impl<'a, B: UsbBus, I, O, R> Interface<'a, B, I, O, R>
where
    B: UsbBus,
    I: InSize,
    O: OutSize,
    R: ReportCount,
{
    pub fn new(usb_alloc: &'a UsbBusAllocator<B>, config: InterfaceConfig<'a, I, O, R>) -> Self {
        Interface {
            id: usb_alloc.interface(),
            in_endpoint: usb_alloc.interrupt(I::Buffer::CAPACITY, config.in_endpoint.poll_interval),
            out_endpoint: config
                .out_endpoint
                .map(|c| usb_alloc.interrupt(O::Buffer::CAPACITY, c.poll_interval)),
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
        if u32::from(report_id) < R::IdleStorage::CAPACITY {
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
impl<'a, B: UsbBus, I, O, R> InterfaceClass<'a> for Interface<'a, B, I, O, R>
where
    B: UsbBus,
    I: InSize,
    O: OutSize,
    R: ReportCount,
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
                    O::Buffer::CAPACITY
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
            return;
        }

        let idx = report_id - 1;
        if u32::from(idx) < R::IdleStorage::CAPACITY {
            self.report_idle.insert(usize::from(idx), value);
            info!("Set report idle for ID{:X} to {:X}", report_id, value);
        } else {
            warn!(
                "Failed to set idle for report id {:X} - max id {:X}",
                report_id,
                R::IdleStorage::CAPACITY
            );
        }
    }
    fn get_idle(&self, report_id: u8) -> u8 {
        if report_id == 0 {
            self.global_idle
        } else {
            let idx = report_id - 1;
            self.get_report_idle(idx).unwrap_or(self.global_idle)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EndpointConfig {
    pub poll_interval: u8,
}

#[must_use = "this `UsbHidInterfaceBuilder` must be assigned or consumed by `::build_interface()`"]
#[derive(Copy, Clone, Debug)]
pub struct InterfaceBuilder<'a, I, O, R>
where
    I: InSize,
    O: OutSize,
    R: ReportCount,
{
    config: InterfaceConfig<'a, I, O, R>,
}

impl<'a, I, O, R> InterfaceBuilder<'a, I, O, R>
where
    I: InSize,
    O: OutSize,
    R: ReportCount,
{
    pub fn new(report_descriptor: &'a [u8]) -> BuilderResult<Self> {
        Ok(InterfaceBuilder {
            config: InterfaceConfig {
                marker: PhantomData,
                report_descriptor,
                report_descriptor_length: u16::try_from(report_descriptor.len())
                    .map_err(|_| UsbHidBuilderError::SliceLengthOverflow)?,
                description: None,
                protocol: InterfaceProtocol::None,
                idle_default: 0,
                out_endpoint: None,
                in_endpoint: EndpointConfig { poll_interval: 20 },
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

    pub fn with_out_endpoint(mut self, poll_interval: MillisDurationU32) -> BuilderResult<Self> {
        self.config.out_endpoint = Some(EndpointConfig {
            poll_interval: u8::try_from(poll_interval.to_millis())
                .map_err(|_| UsbHidBuilderError::ValueOverflow)?,
        });
        Ok(self)
    }

    pub fn without_out_endpoint(mut self) -> Self {
        self.config.out_endpoint = None;
        self
    }

    pub fn in_endpoint(mut self, poll_interval: MillisDurationU32) -> BuilderResult<Self> {
        self.config.in_endpoint = EndpointConfig {
            poll_interval: u8::try_from(poll_interval.to_millis())
                .map_err(|_| UsbHidBuilderError::ValueOverflow)?,
        };
        Ok(self)
    }

    #[must_use]
    pub fn build(self) -> InterfaceConfig<'a, I, O, R> {
        self.config
    }
}

struct IdleManager<R> {
    last_report: Option<R>,
    since_last_report: MillisDurationU32,
}

impl<R> Default for IdleManager<R> {
    fn default() -> Self {
        Self {
            last_report: Option::None,
            since_last_report: 0.millis(),
        }
    }
}

impl<R> IdleManager<R>
where
    R: Eq + Copy,
{
    pub fn report_written(&mut self, report: R) {
        self.last_report = Some(report);
        self.since_last_report = 0.millis();
    }

    pub fn is_duplicate(&self, report: &R) -> bool {
        self.last_report.as_ref() == Some(report)
    }

    /// Call every 1ms
    pub fn tick(&mut self, timeout: MillisDurationU32) -> bool {
        if timeout.ticks() == 0 {
            self.since_last_report = 0.millis();
            return false;
        }

        if self.since_last_report >= timeout {
            self.since_last_report = 0.millis();
            true
        } else {
            self.since_last_report += 1.millis();
            false
        }
    }

    pub fn last_report(&self) -> Option<R> {
        self.last_report
    }
}

pub struct ManagedIdleInterface<'a, B: UsbBus, Report, I, O>
where
    B: UsbBus,
    I: InSize,
    O: OutSize,
{
    interface: Interface<'a, B, I, O, ReportSingle>,
    idle_manager: IdleManager<Report>,
}

#[allow(clippy::inline_always)]
impl<'a, B: UsbBus, Report, I, O> ManagedIdleInterface<'a, B, Report, I, O>
where
    B: UsbBus,
    I: InSize,
    O: OutSize,
{
    fn new(interface: Interface<'a, B, I, O, ReportSingle>) -> Self {
        Self {
            interface,
            idle_manager: IdleManager::default(),
        }
    }
}

#[allow(clippy::inline_always)]
impl<'a, B: UsbBus, Report, I, O, const LEN: usize> ManagedIdleInterface<'a, B, Report, I, O>
where
    Report: Copy + Eq + PackedStruct<ByteArray = [u8; LEN]>,
    B: UsbBus,
    I: InSize,
    O: OutSize,
{
    pub fn write_report(&mut self, report: &Report) -> Result<(), UsbHidError> {
        if self.idle_manager.is_duplicate(report) {
            Err(UsbHidError::Duplicate)
        } else {
            let data = report.pack().map_err(|_| {
                error!("Error packing report");
                UsbHidError::SerializationError
            })?;

            self.interface
                .write_report(&data)
                .map_err(UsbHidError::from)
                .map(|_| {
                    self.idle_manager.report_written(*report);
                })
        }
    }

    pub fn read_report(&mut self, data: &mut [u8]) -> usb_device::Result<usize> {
        self.interface.read_report(data)
    }
}

impl<'a, B: UsbBus, Report, I, O, const LEN: usize> DeviceClass<'a>
    for ManagedIdleInterface<'a, B, Report, I, O>
where
    Report: Copy + Eq + PackedStruct<ByteArray = [u8; LEN]>,
    B: UsbBus,
    I: InSize,
    O: OutSize,
{
    type I = Interface<'a, B, I, O, ReportSingle>;

    fn interface(&mut self) -> &mut Self::I {
        &mut self.interface
    }

    fn reset(&mut self) {
        self.idle_manager = IdleManager::default();
    }

    fn tick(&mut self) -> Result<(), UsbHidError> {
        if !(self.idle_manager.tick(self.interface.global_idle())) {
            Ok(())
        } else if let Some(r) = self.idle_manager.last_report() {
            let data = r.pack().map_err(|_| {
                error!("Error packing report");
                UsbHidError::SerializationError
            })?;
            match self.interface.write_report(&data) {
                Ok(n) => {
                    self.idle_manager.report_written(r);
                    Ok(n)
                }
                Err(UsbError::WouldBlock) => Err(UsbHidError::WouldBlock),
                Err(e) => Err(UsbHidError::UsbError(e)),
            }
            .map(|_| ())
        } else {
            Ok(())
        }
    }
}

pub struct ManagedIdleInterfaceConfig<'a, Report, I, O>
where
    I: InSize,
    O: OutSize,
{
    report: PhantomData<Report>,
    interface_config: InterfaceConfig<'a, I, O, ReportSingle>,
}

impl<'a, Report, I, O> ManagedIdleInterfaceConfig<'a, Report, I, O>
where
    I: InSize,
    O: OutSize,
{
    #[must_use]
    pub fn new(interface_config: InterfaceConfig<'a, I, O, ReportSingle>) -> Self {
        Self {
            interface_config,
            report: PhantomData,
        }
    }
}

impl<'a, B, Report, I, O> UsbAllocatable<'a, B> for ManagedIdleInterfaceConfig<'a, Report, I, O>
where
    B: UsbBus + 'a,
    I: InSize,
    O: OutSize,
{
    type Allocated = ManagedIdleInterface<'a, B, Report, I, O>;

    fn allocate(self, usb_alloc: &'a UsbBusAllocator<B>) -> Self::Allocated {
        ManagedIdleInterface::new(self.interface_config.allocate(usb_alloc))
    }
}
