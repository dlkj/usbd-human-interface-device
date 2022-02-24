use crate::hid_class::descriptor::{
    DescriptorType, HidProtocol, InterfaceProtocol, InterfaceSubClass, USB_CLASS_HID,
};
use crate::hid_class::{BuilderResult, UsbHidBuilderError, UsbPacketSize};
use crate::interface::{InterfaceClass, UsbAllocatable};
use core::cell::RefCell;
use embedded_time::duration::Milliseconds;
use embedded_time::fixed_point::FixedPoint;
use heapless::Vec;
use log::{error, info, trace, warn};
use usb_device::bus::{InterfaceNumber, StringIndex, UsbBus, UsbBusAllocator};
use usb_device::class_prelude::{DescriptorWriter, EndpointIn, EndpointOut};
use usb_device::UsbError;

//todo revisit lifetime for InterfaceConfig. Change to static?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawInterfaceConfig<'a> {
    pub report_descriptor: &'a [u8],
    pub description: Option<&'static str>,
    pub protocol: InterfaceProtocol,
    pub idle_default: u8,
    pub out_endpoint: Option<EndpointConfig>,
    pub in_endpoint: EndpointConfig,
}

pub struct RawInterface<'a, B: UsbBus> {
    id: InterfaceNumber,
    config: RawInterfaceConfig<'a>,
    out_endpoint: Option<EndpointOut<'a, B>>,
    in_endpoint: EndpointIn<'a, B>,
    description_index: Option<StringIndex>,
    protocol: HidProtocol,
    report_idle: heapless::FnvIndexMap<u8, u8, 32>,
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
            report_idle: Default::default(),
            global_idle: self.idle_default,
            control_in_report_buffer: RefCell::new(Default::default()),
            control_out_report_buffer: RefCell::new(Default::default()),
        }
    }
}

impl<'a, B: UsbBus> InterfaceClass<'a> for RawInterface<'a, B> {
    fn report_descriptor(&self) -> &'a [u8] {
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
    fn get_string(&self, index: StringIndex, _lang_id: u16) -> Option<&'static str> {
        self.description_index
            .filter(|&i| i == index)
            .map(|_| self.config.description)
            .flatten()
    }
    fn reset(&mut self) {
        self.protocol = HidProtocol::Report;
        self.global_idle = self.config.idle_default;
        self.report_idle.clear();
        self.control_in_report_buffer.borrow_mut().clear();
        self.control_out_report_buffer.borrow_mut().clear();
    }
    fn set_report(&mut self, data: &[u8]) -> usb_device::Result<()> {
        let mut out_buffer = self.control_out_report_buffer.borrow_mut();
        if !out_buffer.is_empty() {
            trace!("Failed to set report, buffer not empty");
            Err(UsbError::WouldBlock)
        } else {
            match out_buffer.extend_from_slice(data) {
                Err(_) => {
                    error!(
                    "Failed to set report, too large for buffer. Report size {:X}, expected <={:X}",
                    data.len(),
                    &out_buffer.capacity()
                );
                    Err(UsbError::BufferOverflow)
                }
                Ok(_) => {
                    trace!("Set report, {:X} bytes", &out_buffer.len());
                    Ok(())
                }
            }
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
            data[..in_buffer.len()].copy_from_slice(&in_buffer[..]);
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
            self.report_idle.clear();
            info!("Set global idle to {:X}", value);
        } else {
            match self.report_idle.insert(report_id, value) {
                Ok(_) => {
                    info!("Set report idle for ID{:X} to {:X}", report_id, value);
                }
                Err(_) => {
                    warn!(
                        "Failed to set idle for report id {:X}, idle map is full",
                        report_id
                    );
                }
            }
        }
    }
    fn get_idle(&self, report_id: u8) -> u8 {
        if report_id == 0 {
            self.global_idle
        } else {
            *self
                .report_idle
                .get(&report_id)
                .unwrap_or(&self.global_idle)
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

impl<'a, B: UsbBus> RawInterface<'a, B> {
    pub fn protocol(&self) -> HidProtocol {
        self.protocol
    }
    pub fn global_idle(&self) -> Milliseconds {
        Milliseconds((self.global_idle as u32) * 4)
    }
    pub fn report_idle(&self, report_id: u8) -> Option<Milliseconds> {
        if report_id == 0 {
            None
        } else {
            self.report_idle
                .get(&report_id)
                .map(|&i| Milliseconds((i as u32) * 4))
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
            (_, Ok(n)) => Ok(n),
            (Ok(n), _) => Ok(n),
            //non-WouldBlock errors take preference
            (Err(UsbError::WouldBlock), Err(e)) => Err(e),
            (Err(e), Err(UsbError::WouldBlock)) => Err(e),
            (_, Err(e)) => Err(e),
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
    pub fn new(report_descriptor: &'a [u8]) -> Self {
        RawInterfaceBuilder {
            config: RawInterfaceConfig {
                report_descriptor,
                description: None,
                protocol: InterfaceProtocol::None,
                idle_default: 0,
                out_endpoint: None,
                in_endpoint: EndpointConfig {
                    max_packet_size: UsbPacketSize::Bytes8,
                    poll_interval: 20,
                },
            },
        }
    }

    pub fn boot_device(mut self, protocol: InterfaceProtocol) -> Self {
        self.config.protocol = protocol;
        self
    }

    pub fn idle_default<D: Into<Milliseconds>>(mut self, duration: D) -> BuilderResult<Self> {
        let d_ms = duration.into();

        if d_ms == Milliseconds(0_u32) {
            self.config.idle_default = 0;
        } else {
            let scaled_duration = d_ms.integer() / 4;

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
        poll_interval: Milliseconds,
    ) -> BuilderResult<Self> {
        self.config.out_endpoint = Some(EndpointConfig {
            max_packet_size,
            poll_interval: u8::try_from(poll_interval.integer())
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
        poll_interval: Milliseconds,
    ) -> BuilderResult<Self> {
        self.config.in_endpoint = EndpointConfig {
            max_packet_size,
            poll_interval: u8::try_from(poll_interval.integer())
                .map_err(|_| UsbHidBuilderError::ValueOverflow)?,
        };
        Ok(self)
    }

    pub fn build(self) -> RawInterfaceConfig<'a> {
        self.config
    }
}
