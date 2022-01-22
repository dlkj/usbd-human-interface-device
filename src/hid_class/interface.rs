use crate::hid_class::descriptor::*;
use crate::hid_class::*;
use embedded_time::duration::Milliseconds;
use embedded_time::fixed_point::FixedPoint;
use heapless::Vec;
use log::{error, trace, warn};
use usb_device::UsbError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointConfig {
    pub poll_interval: u8,
    pub max_packet_size: UsbPacketSize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterfaceConfig<'a> {
    pub report_descriptor: &'a [u8],
    pub description: Option<&'a str>,
    pub protocol: InterfaceProtocol,
    pub idle_default: u8,
    pub out_endpoint: Option<EndpointConfig>,
    pub in_endpoint: EndpointConfig,
}

#[must_use = "this `UsbHidInterfaceBuilder` must be assigned or consumed by `::build_interface()`"]
#[derive(Clone, Debug)]
pub struct UsbHidInterfaceBuilder<'a, B: UsbBus> {
    class_builder: UsbHidClassBuilder<'a, B>,
    config: InterfaceConfig<'a>,
}

impl<'a, B: UsbBus> UsbHidInterfaceBuilder<'a, B> {
    pub(super) fn new(
        class_builder: UsbHidClassBuilder<'a, B>,
        report_descriptor: &'a [u8],
    ) -> Self {
        UsbHidInterfaceBuilder {
            class_builder,
            config: InterfaceConfig {
                report_descriptor,
                description: None,
                protocol: InterfaceProtocol::None,
                idle_default: 0,
                out_endpoint: None,
                in_endpoint: EndpointConfig {
                    max_packet_size: UsbPacketSize::Size8,
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

    pub fn description(mut self, s: &'a str) -> Self {
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

    pub fn build_interface(mut self) -> BuilderResult<UsbHidClassBuilder<'a, B>> {
        self.class_builder
            .interfaces
            .push(self.config)
            .map_err(|_| UsbHidBuilderError::TooManyInterfaces)?;
        Ok(self.class_builder)
    }
}

pub struct Interface<'a, B: UsbBus> {
    id: InterfaceNumber,
    config: InterfaceConfig<'a>,
    out_endpoint: Option<EndpointOut<'a, B>>,
    in_endpoint: EndpointIn<'a, B>,
    description_index: Option<StringIndex>,
    protocol: HidProtocol,
    report_idle: heapless::FnvIndexMap<u8, u8, 32>,
    global_idle: u8,
    control_in_report_buffer: Vec<u8, 64>,
    control_out_report_buffer: Vec<u8, 64>,
}

impl<'a, B: UsbBus> Interface<'a, B> {
    pub fn config(&self) -> &InterfaceConfig {
        &self.config
    }
}

impl<'a, B: UsbBus> Interface<'a, B> {
    pub(super) fn new(config: InterfaceConfig<'a>, usb_alloc: &'a UsbBusAllocator<B>) -> Self {
        Interface {
            config,
            id: usb_alloc.interface(),
            in_endpoint: usb_alloc.interrupt(
                config.in_endpoint.max_packet_size as u16,
                config.in_endpoint.poll_interval,
            ),
            out_endpoint: config
                .out_endpoint
                .map(|c| usb_alloc.interrupt(c.max_packet_size as u16, c.poll_interval)),
            description_index: config.description.map(|_| usb_alloc.string()),
            //When initialized, all devices default to report protocol - Hid spec 7.2.6 Set_Protocol Request
            protocol: HidProtocol::Report,
            report_idle: Default::default(),
            global_idle: config.idle_default,
            control_in_report_buffer: Default::default(),
            control_out_report_buffer: Default::default(),
        }
    }

    pub fn protocol(&self) -> HidProtocol {
        self.protocol
    }

    pub fn id(&self) -> InterfaceNumber {
        self.id
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

    pub fn write_report(&mut self, data: &[u8]) -> usb_device::Result<usize> {
        let control_result = if self.control_in_report_buffer.is_empty() {
            match self.control_in_report_buffer.extend_from_slice(data) {
                Ok(_) => Ok(data.len()),
                Err(_) => Err(UsbError::BufferOverflow),
            }
        } else {
            Err(UsbError::WouldBlock)
        };

        let endpoint_result = self.in_endpoint.write(data);

        match (control_result, endpoint_result) {
            (_, Ok(n)) => Ok(n),
            (Ok(n), _) => Ok(n),
            (Err(UsbError::WouldBlock), Err(e)) => Err(e),
            (Err(e), Err(UsbError::WouldBlock)) => Err(e),
            (_, Err(e)) => Err(e),
        }
    }

    pub fn read_report(&mut self, data: &mut [u8]) -> usb_device::Result<usize> {
        let ep_result = if let Some(ep) = &self.out_endpoint {
            ep.read(data)
        } else {
            //No endpoint configured
            Err(UsbError::WouldBlock)
        };

        match ep_result {
            Err(UsbError::WouldBlock) => {
                if self.control_out_report_buffer.is_empty() {
                    Err(UsbError::WouldBlock)
                } else if data.len() < self.control_out_report_buffer.len() {
                    Err(UsbError::BufferOverflow)
                } else {
                    let n = self.control_out_report_buffer.len();
                    data[..n].copy_from_slice(&self.control_out_report_buffer);
                    self.control_out_report_buffer.clear();
                    Ok(n)
                }
            }
            _ => ep_result,
        }
    }

    pub fn write_descriptors(&self, writer: &mut DescriptorWriter) -> usb_device::Result<()> {
        writer.interface_alt(
            self.id,
            usb_device::device::DEFAULT_ALTERNATE_SETTING,
            USB_CLASS_HID,
            InterfaceSubClass::from(self.config.protocol) as u8,
            self.config.protocol as u8,
            self.description_index,
        )?;

        //Hid descriptor
        writer.write(
            DescriptorType::Hid as u8,
            &hid_descriptor(self.config.report_descriptor.len())?,
        )?;

        //Endpoint descriptors
        writer.endpoint(&self.in_endpoint)?;
        if let Some(e) = &self.out_endpoint {
            writer.endpoint(e)?;
        }

        Ok(())
    }

    pub fn get_string(&self, index: StringIndex, _lang_id: u16) -> Option<&str> {
        self.description_index
            .filter(|&i| i == index)
            .map(|_| self.config.description)
            .flatten()
    }

    pub fn reset(&mut self) {
        self.protocol = HidProtocol::Report;
        self.global_idle = self.config.idle_default;
        self.report_idle.clear();
        self.control_in_report_buffer.clear();
        self.control_out_report_buffer.clear();
    }

    pub fn set_report(&mut self, data: &[u8]) {
        self.control_out_report_buffer.clear();
        match self.control_out_report_buffer.extend_from_slice(data) {
            Err(_) => {
                error!(
                    "Failed to set report, too large for buffer. Report size {:X}, expected <={:X}",
                    data.len(),
                    &self.control_out_report_buffer.capacity()
                );
            }
            Ok(_) => {
                trace!(
                    "Received report, {:X} bytes",
                    &self.control_out_report_buffer.len()
                );
            }
        }
    }

    //TODO - Should transfer be abstracted away?
    pub fn get_report(&mut self, transfer: ControlIn<B>) {
        if self.control_in_report_buffer.is_empty() {
            trace!("NAK GetReport - empty buffer");

            transfer.reject().ok(); //TODO - read docs, not sure this is correct
        } else {
            if self.control_in_report_buffer.len() != transfer.request().length as usize {
                warn!(
                    "GetReport expected {:X} bytes, sent {:X}",
                    transfer.request().length,
                    self.control_in_report_buffer.len()
                );
            }
            match transfer.accept_with(&self.control_in_report_buffer) {
                Err(e) => error!("Failed to send in report - {:?}", e),
                Ok(_) => {
                    trace!(
                        "Sent report, {:X} bytes",
                        self.control_in_report_buffer.len()
                    )
                }
            }
        }
        self.control_in_report_buffer.clear();
    }

    pub fn set_idle(&mut self, report_id: u8, value: u8) {
        if report_id == 0 {
            self.global_idle = value;
            //"If the lower byte of value is zero, then the idle rate applies to all
            //input reports generated by the device" - HID spec 7.2.4
            self.report_idle.clear();
            trace!("Set global idle to {:X} and cleared report idle map", value);
        } else {
            match self.report_idle.insert(report_id, value) {
                Ok(_) => {
                    trace!("Set report idle for {:X} to {:X}", report_id, value);
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

    pub fn get_idle(&self, report_id: u8) -> u8 {
        if report_id == 0 {
            self.global_idle
        } else {
            *self
                .report_idle
                .get(&report_id)
                .unwrap_or(&self.global_idle)
        }
    }

    pub fn set_protocol(&mut self, protocol: HidProtocol) {
        self.protocol = protocol;
        trace!("Set protocol to {:?}", protocol);
    }
}
