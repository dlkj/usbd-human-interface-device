//! USB Class for implementing Human Interface Devices

use crate::descriptor::{DescriptorType, HidProtocol, HidRequest};
use crate::device::{DeviceClass, DeviceHList};
use crate::interface::{InterfaceClass, UsbAllocatable};
use crate::UsbHidError;
use core::cell::RefCell;
use core::default::Default;
use core::marker::PhantomData;
use frunk::hlist::{HList, Selector};
use frunk::{HCons, HNil, ToMut};
#[allow(clippy::wildcard_imports)]
use usb_device::class_prelude::*;
use usb_device::control::{Recipient, Request};
use usb_device::{control::RequestType, Result};

pub mod prelude {
    //! Prelude for implementing Human Interface Devices
    //!
    //! The purpose of this module is to alleviate imports of structs and enums
    //! required to build and instance a device based on [`UsbHidClass`]:
    //!
    //! ```
    //! # #![allow(unused_imports)]
    //! use usbd_human_interface_device::usb_class::prelude::*;
    //! ```

    pub use crate::descriptor::{HidProtocol, InterfaceProtocol};
    pub use crate::device::DeviceClass;
    pub use crate::interface::{
        InBytes16, InBytes32, InBytes64, InBytes8, InNone, Interface, InterfaceBuilder,
        InterfaceConfig, OutBytes16, OutBytes32, OutBytes64, OutBytes8, OutNone, ReportSingle,
        Reports128, Reports16, Reports32, Reports64, Reports8, UsbAllocatable,
    };
    pub use crate::interface::{ManagedIdleInterface, ManagedIdleInterfaceConfig};
    pub use crate::usb_class::{UsbHidClass, UsbHidClassBuilder};
    pub use crate::UsbHidError;
}

/// [`UsbHidClassBuilder`] error
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbHidBuilderError {
    /// A value is greater than the acceptable range of input values
    ValueOverflow,
    /// A slice of data is longer than permitted
    SliceLengthOverflow,
}

/// Builder for [`UsbHidClass`]
#[must_use = "this `UsbHidClassBuilder` must be assigned or consumed by `::build()`"]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct UsbHidClassBuilder<'a, B, Devices> {
    devices: Devices,
    marker: PhantomData<&'a B>,
}

impl<'a, B> UsbHidClassBuilder<'a, B, HNil> {
    pub fn new() -> Self {
        Self {
            devices: HNil,
            marker: PhantomData,
        }
    }
}

impl<'a, B> Default for UsbHidClassBuilder<'a, B, HNil> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, B: UsbBus, Tail: HList> UsbHidClassBuilder<'a, B, Tail> {
    pub fn add_device<Config, Device>(
        self,
        config: Config,
    ) -> UsbHidClassBuilder<'a, B, HCons<Config, Tail>>
    where
        Config: UsbAllocatable<'a, B, Allocated = Device>,
        Device: DeviceClass<'a>,
    {
        UsbHidClassBuilder {
            devices: self.devices.prepend(config),
            marker: PhantomData,
        }
    }
}

impl<'a, B, Config, Tail> UsbHidClassBuilder<'a, B, HCons<Config, Tail>>
where
    B: UsbBus,
    Tail: UsbAllocatable<'a, B>,
    Config: UsbAllocatable<'a, B>,
{
    pub fn build(
        self,
        usb_alloc: &'a UsbBusAllocator<B>,
    ) -> UsbHidClass<B, HCons<Config::Allocated, Tail::Allocated>> {
        UsbHidClass {
            devices: RefCell::new(self.devices.allocate(usb_alloc)),
            _marker: PhantomData,
        }
    }
}

pub type BuilderResult<B> = core::result::Result<B, UsbHidBuilderError>;

/// USB Human Interface Device class
pub struct UsbHidClass<'a, B, Devices> {
    // Using a RefCell makes it simpler to implement devices as all calls to interfaces are mut
    // this could be removed, but then each usb device would need to implement a non mut borrow
    // of its `RawInterface`.
    devices: RefCell<Devices>,
    _marker: PhantomData<&'a B>,
}

impl<'a, B, Devices: DeviceHList<'a>> UsbHidClass<'a, B, Devices> {
    /// Borrow a single device selected by `T`
    pub fn device<T, Index>(&mut self) -> &mut T
    where
        Devices: Selector<T, Index>,
    {
        self.devices.get_mut().get_mut()
    }

    /// Borrow an [`HList`] of all devices
    pub fn devices(&'a mut self) -> <Devices as ToMut>::Output {
        self.devices.get_mut().to_mut()
    }

    /// Provide a clock tick to allow the tracking of time. Call this every 1ms / at 1KHz
    pub fn tick(&mut self) -> core::result::Result<(), UsbHidError> {
        self.devices.get_mut().tick()
    }
}

impl<'a, B: UsbBus + 'a, Devices> UsbHidClass<'a, B, Devices> {
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

impl<'a, B, Devices> UsbClass<B> for UsbHidClass<'a, B, Devices>
where
    B: UsbBus + 'a,
    Devices: DeviceHList<'a>,
{
    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> Result<()> {
        self.devices.borrow_mut().write_descriptors(writer)?;
        info!("wrote class config descriptor");
        Ok(())
    }

    fn get_string(&self, index: StringIndex, lang_id: u16) -> Option<&str> {
        self.devices.borrow_mut().get_string(index, lang_id)
    }

    fn reset(&mut self) {
        info!("Reset");
        self.devices.get_mut().reset();
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
                    .and_then(|id| self.devices.get_mut().get(id)) else { return };

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
                if let Some(interface) = self.devices.get_mut().get(interface_id) {
                    if request.request == Request::GET_DESCRIPTOR {
                        info!("Get descriptor");
                        Self::get_descriptor(transfer, interface);
                    }
                }
            }

            RequestType::Class => {
                let Some(interface) = self.devices.get_mut().get(interface_id) else {
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

#[cfg(test)]
mod test {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::expect_used)]

    use std::cell::RefCell;
    use std::sync::Mutex;
    use std::vec::Vec;

    use crate::descriptor::USB_CLASS_HID;
    use crate::interface::{InBytes64, InterfaceBuilder, OutBytes64, ReportSingle, Reports8};
    use env_logger::Env;
    use fugit::MillisDurationU32;
    use log::SetLoggerError;
    use packed_struct::prelude::*;
    use usb_device::bus::PollResult;
    use usb_device::prelude::*;
    use usb_device::UsbDirection;

    use super::*;

    fn init_logging() {
        let _: core::result::Result<(), SetLoggerError> =
            env_logger::Builder::from_env(Env::default().default_filter_or("trace"))
                .is_test(true)
                .try_init();
    }

    #[derive(Default)]
    struct UsbTestManager {
        in_buf: Mutex<RefCell<Vec<u8>>>,
        setup_buf: Mutex<RefCell<Vec<u8>>>,
    }

    impl UsbTestManager {
        fn host_write_setup(&self, data: &[u8]) -> Result<()> {
            let buf = self.setup_buf.lock().unwrap();
            if buf.borrow().is_empty() {
                buf.borrow_mut().extend_from_slice(data);
                Ok(())
            } else {
                Err(UsbError::WouldBlock)
            }
        }

        fn host_read_in(&self) -> Vec<u8> {
            self.in_buf.lock().unwrap().take()
        }

        fn has_setup_data(&self) -> bool {
            !self.setup_buf.lock().unwrap().borrow().is_empty()
        }

        fn device_read_setup(&self, data: &mut [u8]) -> Result<usize> {
            let buf = self.setup_buf.lock().unwrap();
            if buf.borrow().is_empty() {
                Err(UsbError::WouldBlock)
            } else {
                let tmp = buf.take();
                data[..tmp.len()].copy_from_slice(&tmp);
                Ok(tmp.len())
            }
        }

        fn device_write(&self, data: &[u8]) -> Result<usize> {
            let buf = self.in_buf.lock().unwrap();
            if buf.borrow().is_empty() {
                buf.borrow_mut().extend_from_slice(data);
                Ok(data.len())
            } else {
                Err(UsbError::WouldBlock)
            }
        }
    }

    struct TestUsbBus<'a> {
        next_ep_index: usize,
        manager: &'a UsbTestManager,
    }

    impl<'a> TestUsbBus<'a> {
        fn new(manager: &'a UsbTestManager) -> Self {
            TestUsbBus {
                next_ep_index: 0,
                manager,
            }
        }
    }

    impl UsbBus for TestUsbBus<'_> {
        fn alloc_ep(
            &mut self,
            ep_dir: UsbDirection,
            _ep_addr: Option<EndpointAddress>,
            _ep_type: EndpointType,
            _max_packet_size: u16,
            _interval: u8,
        ) -> Result<EndpointAddress> {
            let ep = EndpointAddress::from_parts(self.next_ep_index, ep_dir);
            self.next_ep_index += 1;
            Ok(ep)
        }

        fn enable(&mut self) {}
        fn reset(&self) {
            todo!()
        }
        fn set_device_address(&self, _addr: u8) {
            todo!()
        }
        fn write(&self, _ep_addr: EndpointAddress, buf: &[u8]) -> Result<usize> {
            self.manager.device_write(buf)
        }
        fn read(&self, _ep_addr: EndpointAddress, buf: &mut [u8]) -> Result<usize> {
            self.manager.device_read_setup(buf)
        }
        fn set_stalled(&self, _ep_addr: EndpointAddress, _stalled: bool) {}
        fn is_stalled(&self, _ep_addr: EndpointAddress) -> bool {
            todo!()
        }
        fn suspend(&self) {
            todo!()
        }
        fn resume(&self) {
            todo!()
        }
        fn poll(&self) -> PollResult {
            PollResult::Data {
                ep_out: 0,
                ep_in_complete: 1,
                ep_setup: u16::from(self.manager.has_setup_data()),
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PackedStruct)]
    #[packed_struct(endian = "lsb", bit_numbering = "msb0", size_bytes = "8")]
    struct UsbRequest {
        #[packed_field(bits = "0")]
        direction: bool,
        #[packed_field(bits = "1:2")]
        request_type: u8,
        #[packed_field(bits = "4:7")]
        recipient: u8,
        request: u8,
        value: u16,
        index: u16,
        length: u16,
    }

    #[test]
    fn descriptor_ordering_satisfies_boot_spec() {
        init_logging();

        let manager = UsbTestManager::default();

        let usb_alloc = UsbBusAllocator::new(TestUsbBus::new(&manager));

        let mut hid = UsbHidClassBuilder::new()
            .add_device(
                InterfaceBuilder::<InBytes64, OutBytes64, ReportSingle>::new(&[])
                    .unwrap()
                    .build(),
            )
            .build(&usb_alloc);

        let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
            .device_class(USB_CLASS_HID)
            .build();

        // Get Configuration
        manager
            .host_write_setup(
                &UsbRequest {
                    direction: UsbDirection::In != UsbDirection::Out,
                    request_type: RequestType::Standard as u8,
                    recipient: Recipient::Device as u8,
                    request: Request::GET_DESCRIPTOR,
                    value: u16::from(usb_device::descriptor::descriptor_type::CONFIGURATION) << 8,
                    index: 0,
                    length: 0xFFFF,
                }
                .pack()
                .unwrap(),
            )
            .unwrap();

        assert!(usb_dev.poll(&mut [&mut hid]));

        // read multiple transfers then validate config
        let mut data = Vec::new();

        loop {
            let read = manager.host_read_in();
            if read.is_empty() {
                break;
            }
            data.extend_from_slice(&read);
            assert!(usb_dev.poll(&mut [&mut hid]));
        }

        /*
          Expected descriptor order (<https://www.usb.org/sites/default/files/hid1_11.pdf> Appendix F.3):
            Configuration descriptor (other Interface, Endpoint, and Vendor Specific descriptors if required)
            Interface descriptor (with Subclass and Protocol specifying Boot Keyboard)
                Hid descriptor (associated with this Interface)
                Endpoint descriptor (Hid Interrupt In Endpoint)
                    (other Interface, Endpoint, and Vendor Specific descriptors if required)
        */

        let mut it = data.iter();

        let len = *it.next().unwrap();
        assert_eq!(
            *(it.next().unwrap()),
            0x02,
            "Expected Configuration descriptor"
        );
        for _ in 0..(len - 2) {
            it.next().unwrap();
        }

        let len = it.next().unwrap();
        assert_eq!(*it.next().unwrap(), 0x04, "Expected Interface descriptor");
        for _ in 0..(len - 2) {
            it.next().unwrap();
        }

        let len = it.next().unwrap();
        assert_eq!(*(it.next().unwrap()), 0x21, "Expected Hid descriptor");
        for _ in 0..(len - 2) {
            it.next().unwrap();
        }

        while let Some(&len) = it.next() {
            assert_eq!(*(it.next().unwrap()), 0x05, "Expected Endpoint descriptor");

            for _ in 0..(len - 2) {
                it.next().unwrap();
            }
        }

        // Check there isn't any more data
        assert!(it.next().is_none());
    }

    #[test]
    fn get_protocol_default_to_report() {
        init_logging();

        let manager = UsbTestManager::default();
        let usb_alloc = UsbBusAllocator::new(TestUsbBus::new(&manager));

        let mut hid = UsbHidClassBuilder::new()
            .add_device(
                InterfaceBuilder::<InBytes64, OutBytes64, ReportSingle>::new(&[])
                    .unwrap()
                    .build(),
            )
            .build(&usb_alloc);

        let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
            .device_class(USB_CLASS_HID)
            .build();

        // Get protocol
        manager
            .host_write_setup(
                &UsbRequest {
                    direction: UsbDirection::In != UsbDirection::Out,
                    request_type: RequestType::Class as u8,
                    recipient: Recipient::Interface as u8,
                    request: HidRequest::GetProtocol.into(),
                    value: 0x0,
                    index: 0x0,
                    length: 0x1,
                }
                .pack()
                .unwrap(),
            )
            .unwrap();

        assert!(usb_dev.poll(&mut [&mut hid]));

        // read and validate hid_protocol report
        let data = manager.host_read_in();
        assert_eq!(
            data,
            [HidProtocol::Report.into()],
            "Expected protocol to be Report by default"
        );
    }

    #[test]
    fn set_protocol() {
        init_logging();

        let manager = UsbTestManager::default();

        let usb_alloc = UsbBusAllocator::new(TestUsbBus::new(&manager));

        let mut hid = UsbHidClassBuilder::new()
            .add_device(
                InterfaceBuilder::<InBytes64, OutBytes64, ReportSingle>::new(&[])
                    .unwrap()
                    .build(),
            )
            .build(&usb_alloc);

        let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
            .device_class(USB_CLASS_HID)
            .build();

        // Set protocol to boot
        manager
            .host_write_setup(
                &UsbRequest {
                    direction: UsbDirection::In != UsbDirection::In,
                    request_type: RequestType::Class as u8,
                    recipient: Recipient::Interface as u8,
                    request: HidRequest::SetProtocol.into(),
                    value: HidProtocol::Boot as u16,
                    index: 0x0,
                    length: 0x0,
                }
                .pack()
                .unwrap(),
            )
            .unwrap();

        assert!(usb_dev.poll(&mut [&mut hid]));

        // Get protocol
        manager
            .host_write_setup(
                &UsbRequest {
                    direction: UsbDirection::In != UsbDirection::Out,
                    request_type: RequestType::Class as u8,
                    recipient: Recipient::Interface as u8,
                    request: HidRequest::GetProtocol.into(),
                    value: 0x0,
                    index: 0x0,
                    length: 0x1,
                }
                .pack()
                .unwrap(),
            )
            .unwrap();

        assert!(usb_dev.poll(&mut [&mut hid]));

        // read and validate hid_protocol report
        let data = &manager.host_read_in();
        assert_eq!(
            data,
            &[HidProtocol::Boot.into()],
            "Expected protocol to be Boot"
        );
    }

    #[test]
    fn get_protocol_default_post_reset() {
        init_logging();

        let manager = UsbTestManager::default();

        let usb_alloc = UsbBusAllocator::new(TestUsbBus::new(&manager));

        let mut hid = UsbHidClassBuilder::new()
            .add_device(
                InterfaceBuilder::<InBytes64, OutBytes64, ReportSingle>::new(&[])
                    .unwrap()
                    .build(),
            )
            .build(&usb_alloc);

        let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
            .device_class(USB_CLASS_HID)
            .build();

        // Set protocol to boot
        manager
            .host_write_setup(
                &UsbRequest {
                    direction: UsbDirection::In != UsbDirection::In,
                    request_type: RequestType::Class as u8,
                    recipient: Recipient::Interface as u8,
                    request: HidRequest::SetProtocol.into(),
                    value: HidProtocol::Boot as u16,
                    index: 0x0,
                    length: 0x0,
                }
                .pack()
                .unwrap(),
            )
            .unwrap();

        assert!(usb_dev.poll(&mut [&mut hid]));

        // simulate a bus reset after setting protocol to boot
        hid.reset();

        // Get protocol
        manager
            .host_write_setup(
                &UsbRequest {
                    direction: UsbDirection::In != UsbDirection::Out,
                    request_type: RequestType::Class as u8,
                    recipient: Recipient::Interface as u8,
                    request: HidRequest::GetProtocol.into(),
                    value: 0x0,
                    index: 0x0,
                    length: 0x1,
                }
                .pack()
                .unwrap(),
            )
            .unwrap();

        assert!(usb_dev.poll(&mut [&mut hid]));

        // read and validate hid_protocol report
        let data = &manager.host_read_in();
        assert_eq!(
            data,
            &[HidProtocol::Report.into()],
            "Expected protocol to be Report post reset"
        );
    }

    #[test]
    fn get_global_idle_default() {
        const IDLE_DEFAULT: MillisDurationU32 = MillisDurationU32::millis(40);

        init_logging();

        let manager = UsbTestManager::default();

        let usb_alloc = UsbBusAllocator::new(TestUsbBus::new(&manager));

        let mut hid = UsbHidClassBuilder::new()
            .add_device(
                InterfaceBuilder::<InBytes64, OutBytes64, ReportSingle>::new(&[])
                    .unwrap()
                    .idle_default(IDLE_DEFAULT)
                    .unwrap()
                    .build(),
            )
            .build(&usb_alloc);

        let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
            .device_class(USB_CLASS_HID)
            .build();

        // Get idle
        manager
            .host_write_setup(
                &UsbRequest {
                    direction: UsbDirection::In != UsbDirection::Out,
                    request_type: RequestType::Class as u8,
                    recipient: Recipient::Interface as u8,
                    request: HidRequest::GetIdle.into(),
                    value: 0x0,
                    index: 0x0,
                    length: 0x1,
                }
                .pack()
                .unwrap(),
            )
            .unwrap();

        assert!(usb_dev.poll(&mut [&mut hid]));

        // read and validate hid_protocol report
        let data = manager.host_read_in();
        assert_eq!(
            data,
            [u8::try_from(IDLE_DEFAULT.ticks()).unwrap() / 4],
            "Unexpected idle value"
        );
    }

    #[test]
    fn set_global_idle() {
        const IDLE_DEFAULT: MillisDurationU32 = MillisDurationU32::millis(40);
        const IDLE_NEW: MillisDurationU32 = MillisDurationU32::millis(88);

        init_logging();

        let manager = UsbTestManager::default();

        let usb_alloc = UsbBusAllocator::new(TestUsbBus::new(&manager));

        let mut hid = UsbHidClassBuilder::new()
            .add_device(
                InterfaceBuilder::<InBytes64, OutBytes64, ReportSingle>::new(&[])
                    .unwrap()
                    .idle_default(IDLE_DEFAULT)
                    .unwrap()
                    .build(),
            )
            .build(&usb_alloc);

        let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
            .device_class(USB_CLASS_HID)
            .build();

        // Set idle
        manager
            .host_write_setup(
                &UsbRequest {
                    direction: UsbDirection::In != UsbDirection::In,
                    request_type: RequestType::Class as u8,
                    recipient: Recipient::Interface as u8,
                    request: HidRequest::SetIdle.into(),
                    value: (u16::try_from(IDLE_NEW.to_millis()).unwrap() / 4) << 8,
                    index: 0x0,
                    length: 0x0,
                }
                .pack()
                .unwrap(),
            )
            .unwrap();

        assert!(usb_dev.poll(&mut [&mut hid]));

        // Get idle
        manager
            .host_write_setup(
                &UsbRequest {
                    direction: UsbDirection::In != UsbDirection::Out,
                    request_type: RequestType::Class as u8,
                    recipient: Recipient::Interface as u8,
                    request: HidRequest::GetIdle.into(),
                    value: 0x0,
                    index: 0x0,
                    length: 0x1,
                }
                .pack()
                .unwrap(),
            )
            .unwrap();

        assert!(usb_dev.poll(&mut [&mut hid]));

        // read and validate hid_protocol report
        let data = manager.host_read_in();
        assert_eq!(
            data,
            [u8::try_from(IDLE_NEW.ticks()).unwrap() / 4],
            "Unexpected idle value"
        );
    }

    #[test]
    fn get_global_idle_default_post_reset() {
        const IDLE_DEFAULT: MillisDurationU32 = MillisDurationU32::millis(40);
        const IDLE_NEW: MillisDurationU32 = MillisDurationU32::millis(88);

        init_logging();

        let manager = UsbTestManager::default();

        let usb_alloc = UsbBusAllocator::new(TestUsbBus::new(&manager));

        let mut hid = UsbHidClassBuilder::new()
            .add_device(
                InterfaceBuilder::<InBytes64, OutBytes64, ReportSingle>::new(&[])
                    .unwrap()
                    .idle_default(IDLE_DEFAULT)
                    .unwrap()
                    .build(),
            )
            .build(&usb_alloc);

        let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
            .device_class(USB_CLASS_HID)
            .build();

        // Set idle
        manager
            .host_write_setup(
                &UsbRequest {
                    direction: UsbDirection::In != UsbDirection::In,
                    request_type: RequestType::Class as u8,
                    recipient: Recipient::Interface as u8,
                    request: HidRequest::SetIdle.into(),
                    value: (u16::try_from(IDLE_NEW.to_millis()).unwrap() / 4) << 8,
                    index: 0x0,
                    length: 0x0,
                }
                .pack()
                .unwrap(),
            )
            .unwrap();

        assert!(usb_dev.poll(&mut [&mut hid]));

        hid.reset();

        // Get idle
        manager
            .host_write_setup(
                &UsbRequest {
                    direction: UsbDirection::In != UsbDirection::Out,
                    request_type: RequestType::Class as u8,
                    recipient: Recipient::Interface as u8,
                    request: HidRequest::GetIdle.into(),
                    value: 0x0,
                    index: 0x0,
                    length: 0x1,
                }
                .pack()
                .unwrap(),
            )
            .unwrap();

        assert!(usb_dev.poll(&mut [&mut hid]));

        // read and validate hid_protocol report
        let data = manager.host_read_in();
        assert_eq!(
            data,
            [u8::try_from(IDLE_DEFAULT.ticks()).unwrap() / 4],
            "Unexpected idle value"
        );
    }

    #[test]
    fn get_report_idle_default() {
        const IDLE_DEFAULT: MillisDurationU32 = MillisDurationU32::millis(40);
        const REPORT_ID: u8 = 0xAB;

        init_logging();

        let manager = UsbTestManager::default();

        let usb_bus = TestUsbBus::new(&manager);

        let usb_alloc = UsbBusAllocator::new(usb_bus);

        let mut hid = UsbHidClassBuilder::new()
            .add_device(
                InterfaceBuilder::<InBytes64, OutBytes64, ReportSingle>::new(&[])
                    .unwrap()
                    .idle_default(IDLE_DEFAULT)
                    .unwrap()
                    .build(),
            )
            .build(&usb_alloc);

        let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
            .device_class(USB_CLASS_HID)
            .build();

        // Get idle
        manager
            .host_write_setup(
                &UsbRequest {
                    direction: UsbDirection::In != UsbDirection::Out,
                    request_type: RequestType::Class as u8,
                    recipient: Recipient::Interface as u8,
                    request: HidRequest::GetIdle.into(),
                    value: u16::from(REPORT_ID),
                    index: 0x0,
                    length: 0x1,
                }
                .pack()
                .unwrap(),
            )
            .unwrap();

        assert!(usb_dev.poll(&mut [&mut hid]));

        // read and validate hid_protocol report
        let data = manager.host_read_in();
        assert_eq!(
            data,
            [u8::try_from(IDLE_DEFAULT.ticks()).unwrap() / 4],
            "Unexpected idle value"
        );
    }

    #[test]
    fn set_report_idle() {
        const IDLE_DEFAULT: MillisDurationU32 = MillisDurationU32::millis(40);
        const IDLE_NEW: MillisDurationU32 = MillisDurationU32::millis(88);
        const REPORT_ID: u8 = 0x4;

        init_logging();

        let manager = UsbTestManager::default();

        let usb_alloc = UsbBusAllocator::new(TestUsbBus::new(&manager));

        let mut hid = UsbHidClassBuilder::new()
            .add_device(
                InterfaceBuilder::<InBytes64, OutBytes64, Reports8>::new(&[])
                    .unwrap()
                    .idle_default(IDLE_DEFAULT)
                    .unwrap()
                    .build(),
            )
            .build(&usb_alloc);

        let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
            .device_class(USB_CLASS_HID)
            .build();

        // Set report idle
        manager
            .host_write_setup(
                &UsbRequest {
                    direction: UsbDirection::In != UsbDirection::In,
                    request_type: RequestType::Class as u8,
                    recipient: Recipient::Interface as u8,
                    request: HidRequest::SetIdle.into(),
                    value: (u16::try_from(IDLE_NEW.to_millis()).unwrap() / 4) << 8
                        | u16::from(REPORT_ID),
                    index: 0x0,
                    length: 0x0,
                }
                .pack()
                .unwrap(),
            )
            .unwrap();

        assert!(usb_dev.poll(&mut [&mut hid]));

        // Get report idle
        manager
            .host_write_setup(
                &UsbRequest {
                    direction: UsbDirection::In != UsbDirection::Out,
                    request_type: RequestType::Class as u8,
                    recipient: Recipient::Interface as u8,
                    request: HidRequest::GetIdle.into(),
                    value: u16::from(REPORT_ID),
                    index: 0x0,
                    length: 0x1,
                }
                .pack()
                .unwrap(),
            )
            .unwrap();

        assert!(usb_dev.poll(&mut [&mut hid]));

        // read and validate hid_protocol report
        let data = manager.host_read_in();
        assert_eq!(
            data,
            [u8::try_from(IDLE_NEW.ticks()).unwrap() / 4],
            "Unexpected report idle value"
        );

        // Get global idle
        manager
            .host_write_setup(
                &UsbRequest {
                    direction: UsbDirection::In != UsbDirection::Out,
                    request_type: RequestType::Class as u8,
                    recipient: Recipient::Interface as u8,
                    request: HidRequest::GetIdle.into(),
                    value: 0x0,
                    index: 0x0,
                    length: 0x1,
                }
                .pack()
                .unwrap(),
            )
            .unwrap();

        assert!(usb_dev.poll(&mut [&mut hid]));

        // read and validate hid_protocol report
        let data = manager.host_read_in();
        assert_eq!(
            data,
            [u8::try_from(IDLE_DEFAULT.ticks()).unwrap() / 4],
            "Unexpected global idle value"
        );
    }

    #[test]
    fn set_report_idle_no_reports() {
        const IDLE_DEFAULT: MillisDurationU32 = MillisDurationU32::millis(40);
        const IDLE_NEW: MillisDurationU32 = MillisDurationU32::millis(88);
        const REPORT_ID: u16 = 0x4;

        init_logging();

        let manager = UsbTestManager::default();

        let usb_alloc = UsbBusAllocator::new(TestUsbBus::new(&manager));

        let mut hid = UsbHidClassBuilder::new()
            .add_device(
                InterfaceBuilder::<InBytes64, OutBytes64, ReportSingle>::new(&[])
                    .unwrap()
                    .idle_default(IDLE_DEFAULT)
                    .unwrap()
                    .build(),
            )
            .build(&usb_alloc);

        let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
            .device_class(USB_CLASS_HID)
            .build();

        // Set report idle - this is a nop due to lack of report storage in the device (`SingleReport`)
        manager
            .host_write_setup(
                &UsbRequest {
                    direction: UsbDirection::In != UsbDirection::In,
                    request_type: RequestType::Class as u8,
                    recipient: Recipient::Interface as u8,
                    request: HidRequest::SetIdle as u8,
                    value: (u16::try_from(IDLE_NEW.to_millis()).unwrap() / 4) << 8 | REPORT_ID,
                    index: 0x0,
                    length: 0x0,
                }
                .pack()
                .unwrap(),
            )
            .unwrap();

        assert!(usb_dev.poll(&mut [&mut hid]));

        // Get report idle
        manager
            .host_write_setup(
                &UsbRequest {
                    direction: UsbDirection::In != UsbDirection::Out,
                    request_type: RequestType::Class as u8,
                    recipient: Recipient::Interface as u8,
                    request: HidRequest::GetIdle as u8,
                    value: REPORT_ID,
                    index: 0x0,
                    length: 0x1,
                }
                .pack()
                .unwrap(),
            )
            .unwrap();

        assert!(usb_dev.poll(&mut [&mut hid]));

        // read and validate hid_protocol report
        let data = manager.host_read_in();
        assert_eq!(
            data,
            [u8::try_from(IDLE_DEFAULT.ticks()).unwrap() / 4],
            "Unexpected report idle value"
        );

        // Get global idle
        manager
            .host_write_setup(
                &UsbRequest {
                    direction: UsbDirection::In != UsbDirection::Out,
                    request_type: RequestType::Class as u8,
                    recipient: Recipient::Interface as u8,
                    request: HidRequest::GetIdle as u8,
                    value: 0x0,
                    index: 0x0,
                    length: 0x1,
                }
                .pack()
                .unwrap(),
            )
            .unwrap();

        assert!(usb_dev.poll(&mut [&mut hid]));

        // read and validate hid_protocol report
        let data = manager.host_read_in();
        assert_eq!(
            data,
            [u8::try_from(IDLE_DEFAULT.ticks()).unwrap() / 4],
            "Unexpected global idle value"
        );
    }

    #[test]
    fn get_report_idle_default_post_reset() {
        const REPORT_ID: u8 = 0x4;
        const IDLE_DEFAULT: MillisDurationU32 = MillisDurationU32::millis(40);
        const IDLE_NEW: MillisDurationU32 = MillisDurationU32::millis(88);

        init_logging();

        let manager = UsbTestManager::default();

        let usb_alloc = UsbBusAllocator::new(TestUsbBus::new(&manager));

        let mut hid = UsbHidClassBuilder::new()
            .add_device(
                InterfaceBuilder::<InBytes64, OutBytes64, ReportSingle>::new(&[])
                    .unwrap()
                    .idle_default(IDLE_DEFAULT)
                    .unwrap()
                    .build(),
            )
            .build(&usb_alloc);

        let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
            .device_class(USB_CLASS_HID)
            .build();

        // Set report idle
        manager
            .host_write_setup(
                &UsbRequest {
                    direction: UsbDirection::In != UsbDirection::In,
                    request_type: RequestType::Class as u8,
                    recipient: Recipient::Interface as u8,
                    request: HidRequest::SetIdle.into(),
                    value: (u16::try_from(IDLE_NEW.to_millis()).unwrap() / 4) << 8
                        | u16::from(REPORT_ID),
                    index: 0x0,
                    length: 0x0,
                }
                .pack()
                .unwrap(),
            )
            .unwrap();

        assert!(usb_dev.poll(&mut [&mut hid]));

        hid.reset();

        // Get report idle
        manager
            .host_write_setup(
                &UsbRequest {
                    direction: UsbDirection::In != UsbDirection::Out,
                    request_type: RequestType::Class as u8,
                    recipient: Recipient::Interface as u8,
                    request: HidRequest::GetIdle.into(),
                    value: u16::from(REPORT_ID),
                    index: 0x0,
                    length: 0x1,
                }
                .pack()
                .unwrap(),
            )
            .unwrap();

        assert!(usb_dev.poll(&mut [&mut hid]));

        // read and validate hid_protocol report
        let data = manager.host_read_in();
        assert_eq!(
            data,
            [u8::try_from(IDLE_DEFAULT.ticks()).unwrap() / 4],
            "Unexpected report idle value"
        );
    }
}
