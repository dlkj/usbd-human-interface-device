use crate::hid_class::descriptor::{
    DescriptorType, HidProtocol, InterfaceProtocol, InterfaceSubClass, COUNTRY_CODE_NOT_SUPPORTED,
    SPEC_VERSION_1_11, USB_CLASS_HID,
};
use crate::hid_class::{BuilderResult, UsbHidBuilderError, UsbPacketSize};
use crate::interface::{InterfaceClass, UsbAllocatable};
use core::cell::RefCell;
use fugit::{ExtU32, MillisDurationU32};
use heapless::Vec;
use option_block::Block32;
use packed_struct::PackedStruct;
use usb_device::bus::{InterfaceNumber, StringIndex, UsbBus, UsbBusAllocator};
#[allow(clippy::wildcard_imports)]
use usb_device::class_prelude::*;
use usb_device::UsbError;

use super::HidDescriptorBody;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawInterfaceConfig<'a> {
    pub report_descriptor: &'a [u8],
    pub report_descriptor_length: u16,
    pub description: Option<&'a str>,
    pub protocol: InterfaceProtocol,
    pub idle_default: u8,
    pub out_endpoint: Option<EndpointConfig>,
    pub in_endpoint: EndpointConfig,
}

// TODO: make configurable, size depends on number of reports for given interface,
// in most cases Block8 (max 8 reports) would be enough (size 9B vs 36B for Block32)
type ReportIdleArray = Block32<u8>;

pub struct RawInterface<'a, B: UsbBus> {
    id: InterfaceNumber,
    config: RawInterfaceConfig<'a>,
    out_endpoint: Option<EndpointOut<'a, B>>,
    in_endpoint: EndpointIn<'a, B>,
    description_index: Option<StringIndex>,
    protocol: HidProtocol,
    report_idle: ReportIdleArray,
    global_idle: u8,
    control_in_report_buffer: RefCell<Vec<u8, 64>>,
    control_out_report_buffer: RefCell<Vec<u8, 64>>,
}

impl<'a, B: UsbBus + 'a> UsbAllocatable<'a, B> for RawInterfaceConfig<'a> {
    type Allocated = RawInterface<'a, B>;

    fn allocate(self, usb_alloc: &'a UsbBusAllocator<B>) -> Self::Allocated {
        RawInterface {
            config: self,
            id: usb_alloc.interface(),
            in_endpoint: usb_alloc.interrupt(
                self.in_endpoint.max_packet_size as u16,
                self.in_endpoint.poll_interval,
            ),
            out_endpoint: self
                .out_endpoint
                .map(|c| usb_alloc.interrupt(c.max_packet_size as u16, c.poll_interval)),
            description_index: self.description.map(|_| usb_alloc.string()),
            //When initialized, all devices default to report protocol - Hid spec 7.2.6 Set_Protocol Request
            protocol: HidProtocol::Report,
            report_idle: Block32::default(),
            global_idle: self.idle_default,
            control_in_report_buffer: RefCell::new(Vec::default()),
            control_out_report_buffer: RefCell::new(Vec::default()),
        }
    }
}

impl<'a, B: UsbBus> InterfaceClass<'a> for RawInterface<'a, B> {
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
            InterfaceSubClass::from(self.config.protocol) as u8,
            self.config.protocol as u8,
            self.description_index,
        )?;

        //Hid descriptor
        writer.write(DescriptorType::Hid as u8, &self.hid_descriptor_body())?;

        //Endpoint descriptors
        writer.endpoint(&self.in_endpoint)?;
        if let Some(e) = &self.out_endpoint {
            writer.endpoint(e)?;
        }

        Ok(())
    }
    fn get_string(&self, index: StringIndex, _lang_id: u16) -> Option<&'_ str> {
        self.description_index
            .filter(|&i| i == index)
            .and(self.config.description)
    }
    fn reset(&mut self) {
        self.protocol = HidProtocol::Report;
        self.global_idle = self.config.idle_default;
        self.clear_report_idle();
        self.control_in_report_buffer.borrow_mut().clear();
        self.control_out_report_buffer.borrow_mut().clear();
    }
    fn set_report(&mut self, data: &[u8]) -> usb_device::Result<()> {
        let mut out_buffer = self.control_out_report_buffer.borrow_mut();
        if out_buffer.is_empty() {
            if out_buffer.extend_from_slice(data).is_ok() {
                trace!("Set report, {} bytes", &out_buffer.len());
                Ok(())
            } else {
                error!(
                    "Failed to set report, too large for buffer. Report size {}, expected <={}",
                    data.len(),
                    &out_buffer.capacity()
                );
                Err(UsbError::BufferOverflow)
            }
        } else {
            trace!("Failed to set report, buffer not empty");
            Err(UsbError::WouldBlock)
        }
    }

    fn get_report(&mut self, data: &mut [u8]) -> usb_device::Result<usize> {
        let in_buffer = self.control_in_report_buffer.borrow();
        if in_buffer.is_empty() {
            trace!("GetReport would block, empty buffer");
            Err(UsbError::WouldBlock)
        } else if data.len() < in_buffer.len() {
            error!("GetReport failed, buffer too short");
            Err(UsbError::BufferOverflow)
        } else {
            data[..in_buffer.len()].copy_from_slice(&in_buffer);
            Ok(in_buffer.len())
        }
    }

    fn get_report_ack(&mut self) -> usb_device::Result<()> {
        let mut in_buffer = self.control_in_report_buffer.borrow_mut();
        if in_buffer.is_empty() {
            error!("GetReport ACK failed, empty buffer");
            Err(UsbError::WouldBlock)
        } else {
            in_buffer.clear();
            Ok(())
        }
    }

    fn set_idle(&mut self, report_id: u8, value: u8) {
        if report_id == 0 {
            self.global_idle = value;
            //"If the lower byte of value is zero, then the idle rate applies to all
            //input reports generated by the device" - HID spec 7.2.4
            self.clear_report_idle();
            info!("Set global idle to {}", value);
        } else if u32::from(report_id) < ReportIdleArray::CAPACITY {
            self.report_idle.insert(report_id as usize, value);
            info!("Set report idle for ID{} to {}", report_id, value);
        } else {
            warn!(
                "Failed to set idle for report id {} - max id {}",
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
}

impl<'a, B: UsbBus> RawInterface<'a, B> {
    fn clear_report_idle(&mut self) {
        self.report_idle = Block32::default();
    }
    fn get_report_idle(&self, report_id: u8) -> Option<u8> {
        if u32::from(report_id) < ReportIdleArray::CAPACITY {
            self.report_idle.get(report_id as usize).copied()
        } else {
            None
        }
    }
    pub fn protocol(&self) -> HidProtocol {
        self.protocol
    }
    pub fn global_idle(&self) -> MillisDurationU32 {
        (u32::from(self.global_idle) * 4).millis()
    }
    pub fn report_idle(&self, report_id: u8) -> Option<MillisDurationU32> {
        if report_id == 0 {
            None
        } else {
            self.get_report_idle(report_id)
                .map(|i| (u32::from(i) * 4).millis())
        }
    }
    pub fn write_report(&self, data: &[u8]) -> usb_device::Result<usize> {
        //Try to write report to the report buffer for the config endpoint
        let mut in_buffer = self.control_in_report_buffer.borrow_mut();
        let control_result = if in_buffer.is_empty() {
            match in_buffer.extend_from_slice(data) {
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
    pub fn read_report(&self, data: &mut [u8]) -> usb_device::Result<usize> {
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
                let mut out_buffer = self.control_out_report_buffer.borrow_mut();
                if out_buffer.is_empty() {
                    Err(UsbError::WouldBlock)
                } else if data.len() < out_buffer.len() {
                    Err(UsbError::BufferOverflow)
                } else {
                    let n = out_buffer.len();
                    data[..n].copy_from_slice(&out_buffer);
                    out_buffer.clear();
                    Ok(n)
                }
            }
            _ => ep_result,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointConfig {
    pub poll_interval: u8,
    pub max_packet_size: UsbPacketSize,
}

#[must_use = "this `UsbHidInterfaceBuilder` must be assigned or consumed by `::build_interface()`"]
#[derive(Clone, Debug)]
pub struct RawInterfaceBuilder<'a> {
    config: RawInterfaceConfig<'a>,
}

impl<'a> RawInterfaceBuilder<'a> {
    pub fn new(report_descriptor: &'a [u8]) -> BuilderResult<Self> {
        Ok(RawInterfaceBuilder {
            config: RawInterfaceConfig {
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

    pub fn description(mut self, s: &'static str) -> Self {
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
    pub fn build(self) -> RawInterfaceConfig<'a> {
        self.config
    }
}
