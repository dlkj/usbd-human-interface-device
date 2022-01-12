//!Implements UsbHidClass representing a USB Human Interface Device

use embedded_time::duration::*;
use log::{error, info, trace};
use packed_struct::prelude::*;
use usb_device::class_prelude::*;
use usb_device::Result;

const USB_CLASS_HID: u8 = 0x03;
const SPEC_VERSION_1_11: u16 = 0x0111; //1.11 in BCD
const COUNTRY_CODE_NOT_SUPPORTED: u8 = 0x0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PrimitiveEnum)]
#[repr(u8)]
pub enum Request {
    GetReport = 0x01,
    GetIdle = 0x02,
    GetProtocol = 0x03,
    SetReport = 0x09,
    SetIdle = 0x0A,
    SetProtocol = 0x0B,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum InterfaceProtocol {
    None = 0x00,
    Keyboard = 0x01,
    Mouse = 0x02,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PrimitiveEnum)]
#[repr(u8)]
pub enum DescriptorType {
    Hid = 0x21,
    Report = 0x22,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum InterfaceSubClass {
    None = 0x00,
    Boot = 0x01,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PrimitiveEnum)]
#[repr(u8)]
pub enum HidProtocol {
    Boot = 0x00,
    Report = 0x01,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PackedStruct)]
#[packed_struct(endian = "lsb", size_bytes = 7)]
struct HidDescriptorBody {
    bcd_hid: u16,
    country_code: u8,
    num_descriptors: u8,
    #[packed_field(ty = "enum", size_bytes = "1")]
    descriptor_type: DescriptorType,
    descriptor_length: u16,
}

pub trait HidConfig {
    /*todo
     * Replace with builder calling a private constructor on UsbHidClass
     * Independent config of endpoints
     * Make out endpoint optional
     * Default Idle config
     * Link boot and protocol
     */

    fn packet_size(&self) -> u8 {
        8
    }
    fn poll_interval(&self) -> Milliseconds {
        Milliseconds(10)
    }
    fn report_descriptor(&self) -> &'static [u8];
    fn interface_protocol(&self) -> InterfaceProtocol {
        InterfaceProtocol::None
    }
    fn interface_sub_class(&self) -> InterfaceSubClass {
        InterfaceSubClass::None
    }
    fn interface_name(&self) -> &str;
    fn reset(&mut self) {}
}

/// Implements the generic Hid Device
pub struct UsbHidClass<'a, B: UsbBus, C: HidConfig> {
    hid_class: C,
    interface_number: InterfaceNumber,
    out_endpoint: EndpointOut<'a, B>,
    in_endpoint: EndpointIn<'a, B>,
    string_index: StringIndex,
    protocol: HidProtocol,
}

impl<B: UsbBus, C: HidConfig> UsbHidClass<'_, B, C> {
    pub fn new(usb_alloc: &'_ UsbBusAllocator<B>, hid_class: C) -> UsbHidClass<'_, B, C> {
        let interval_ms = hid_class.poll_interval().integer() as u8;
        let interface_number = usb_alloc.interface();

        info!("Assigned interface {:X}", u8::from(interface_number));

        UsbHidClass {
            hid_class,
            interface_number,
            //todo make packet size configurable
            //todo make endpoints independently configurable
            //todo make endpoints optional
            out_endpoint: usb_alloc.interrupt(8, interval_ms),
            in_endpoint: usb_alloc.interrupt(8, interval_ms),
            string_index: usb_alloc.string(),
            protocol: HidProtocol::Report, //When initialized, all devices default to report protocol - Hid spec 7.2.6 Set_Protocol Request
        }
    }

    fn hid_descriptor(&self) -> Result<[u8; 7]> {
        let descriptor_len = self.hid_class.report_descriptor().len();

        if descriptor_len > u16::max_value() as usize {
            error!(
                "Report descriptor for interface {:X}, length {} too long to fit in u16",
                u8::from(self.interface_number),
                descriptor_len
            );
            Err(UsbError::InvalidState)
        } else {
            Ok(HidDescriptorBody {
                bcd_hid: SPEC_VERSION_1_11,
                country_code: COUNTRY_CODE_NOT_SUPPORTED,
                num_descriptors: 1,
                descriptor_type: DescriptorType::Report,
                descriptor_length: descriptor_len as u16,
            }
            .pack()
            .map_err(|e| {
                error!("Failed to pack HidDescriptor {}", e);
                UsbError::InvalidState
            })?)
        }
    }

    pub fn protocol(&self) -> HidProtocol {
        self.protocol
    }

    pub fn write_report(&self, data: &[u8]) -> Result<usize> {
        self.in_endpoint.write(data)
    }

    pub fn read_report(&self, data: &mut [u8]) -> Result<usize> {
        self.out_endpoint.read(data)
    }

    fn control_in_get_descriptors(&mut self, transfer: ControlIn<B>) {
        let request: &control::Request = transfer.request();
        match DescriptorType::from_primitive((request.value >> 8) as u8) {
            Some(DescriptorType::Report) => {
                transfer
                    .accept_with_static(self.hid_class.report_descriptor())
                    .unwrap_or_else(|e| {
                        error!(
                            "interface {:X} - Failed to send report descriptor - {:?}",
                            u8::from(self.interface_number),
                            e
                        )
                    });
            }
            Some(DescriptorType::Hid) => match self.hid_descriptor() {
                Err(e) => {
                    error!(
                        "interface {:X} - Failed to generate Hid descriptor - {:?}",
                        u8::from(self.interface_number),
                        e
                    );
                    transfer.reject().ok();
                }
                Ok(hid_descriptor) => {
                    let mut buffer = [0; 9];
                    buffer[0] = buffer.len() as u8;
                    buffer[1] = DescriptorType::Hid as u8;
                    (&mut buffer[2..]).copy_from_slice(&hid_descriptor);
                    transfer.accept_with(&buffer).unwrap_or_else(|e| {
                        error!(
                            "interface {:X} - Failed to send Hid descriptor - {:?}",
                            u8::from(self.interface_number),
                            e
                        );
                    });
                }
            },
            _ => {
                trace!(
                    "interface {:X} - unsupported - request type: {:?}, request: {:X}, value: {:X}",
                    u8::from(self.interface_number),
                    request.request_type,
                    request.request,
                    request.value
                );
            }
        }
    }
}

impl<B: UsbBus, C: HidConfig> UsbClass<B> for UsbHidClass<'_, B, C> {
    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> Result<()> {
        writer.interface_alt(
            self.interface_number,
            usb_device::device::DEFAULT_ALTERNATE_SETTING,
            USB_CLASS_HID,
            self.hid_class.interface_sub_class() as u8,
            self.hid_class.interface_protocol() as u8,
            Some(self.string_index),
        )?;

        //Hid descriptor
        writer.write(DescriptorType::Hid as u8, &self.hid_descriptor()?)?;

        //Endpoint descriptors
        writer.endpoint(&self.out_endpoint)?;
        writer.endpoint(&self.in_endpoint)?;

        Ok(())
    }

    fn get_string(&self, index: StringIndex, _lang_id: u16) -> Option<&str> {
        if index == self.string_index {
            Some(self.hid_class.interface_name())
        } else {
            None
        }
    }

    fn control_in(&mut self, transfer: ControlIn<B>) {
        let request: &control::Request = transfer.request();
        //only respond to requests for this interface
        if !(request.recipient == control::Recipient::Interface
            && request.index == u8::from(self.interface_number) as u16)
        {
            return;
        }
        trace!(
            "ctrl_in: request type: {:?}, request: {:X}, value: {:X}",
            request.request_type,
            request.request,
            request.value
        );

        match request.request_type {
            control::RequestType::Standard => {
                if request.request == control::Request::GET_DESCRIPTOR {
                    self.control_in_get_descriptors(transfer);
                }
            }

            control::RequestType::Class => {
                match Request::from_primitive(request.request) {
                    Some(Request::GetReport) => {
                        // Not supported - data reports handled via interrupt endpoints
                        transfer.reject().ok(); // Not supported
                    }
                    Some(Request::GetIdle) => {
                        transfer.reject().ok(); // Not supported
                    }
                    Some(Request::GetProtocol) => {
                        let data = [self.protocol as u8];
                        transfer
                            .accept_with(&data)
                            .unwrap_or_else(|e| error!("Failed to send protocol - {:?}", e));
                    }
                    _ => {
                        error!(
                            "Unsupported control_in request type: {:?}, request: {:X}, value: {:X}",
                            request.request_type, request.request, request.value
                        );
                        transfer.reject().ok(); // Not supported
                    }
                }
            }
            _ => {}
        }
    }

    fn control_out(&mut self, transfer: ControlOut<B>) {
        let request: &control::Request = transfer.request();

        //only respond to Class requests for this interface
        if !(request.request_type == control::RequestType::Class
            && request.recipient == control::Recipient::Interface
            && request.index == u8::from(self.interface_number) as u16)
        {
            return;
        }

        trace!(
            "ctrl_out: request type: {:?}, request: {:X}, value: {:X}",
            request.request_type,
            request.request,
            request.value
        );

        match Request::from_primitive(request.request) {
            Some(Request::SetIdle) => {
                transfer.accept().ok(); //Not supported
            }
            Some(Request::SetReport) => {
                // Not supported - data reports handled via interrupt endpoints
                transfer.reject().ok();
            }
            Some(Request::SetProtocol) => {
                if let Some(protocol) = HidProtocol::from_primitive((request.value & 0xFF) as u8) {
                    self.protocol = protocol;
                    transfer.accept().ok();
                } else {
                    error!(
                        "Ctrl_out - Unsupported protocol value:{:X}, Unable to set protocol.",
                        request.value
                    );
                    transfer.reject().ok();
                }
            }
            _ => {
                error!(
                    "Unsupported control_out request type: {:?}, request: {:X}, value: {:X}",
                    request.request_type, request.request, request.value
                );
                transfer.reject().ok();
            }
        }
    }

    fn reset(&mut self) {
        //Default to report protocol on reset
        self.protocol = HidProtocol::Report;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::sync::Mutex;
    use std::vec::Vec;
    use usb_device::bus::*;
    use usb_device::prelude::*;
    use usb_device::*;

    struct TestUsbBus<'a, F> {
        next_ep_index: usize,
        read_data: &'a [&'a [u8]],
        write_val: F,
        inner: Mutex<RefCell<TestUsbBusInner>>,
    }
    struct TestUsbBusInner {
        next_read_data: usize,
        write_data: Vec<u8>,
    }

    impl<'a, F> TestUsbBus<'a, F> {
        fn new(read_data: &'a [&'a [u8]], write_val: F) -> Self {
            TestUsbBus {
                next_ep_index: 0,
                read_data,
                write_val,
                inner: Mutex::new(RefCell::new(TestUsbBusInner {
                    write_data: Vec::new(),
                    next_read_data: 0,
                })),
            }
        }
    }

    impl<F> UsbBus for TestUsbBus<'_, F>
    where
        F: core::marker::Sync + Fn(&Vec<u8>),
    {
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
            todo!();
        }
        fn set_device_address(&self, _addr: u8) {
            todo!();
        }
        fn write(&self, _ep_addr: EndpointAddress, buf: &[u8]) -> Result<usize> {
            let inner_ref = self.inner.lock().unwrap();
            let mut inner = inner_ref.borrow_mut();

            inner.write_data.extend_from_slice(buf);

            if buf.len() < 8 && !inner.write_data.is_empty() {
                //if we get less than a full buffer, the write is complete, validate the buffer
                (self.write_val)(&inner.write_data)
            }

            Ok(buf.len())
        }
        fn read(&self, _ep_addr: EndpointAddress, buf: &mut [u8]) -> Result<usize> {
            let inner_ref = self.inner.lock().unwrap();
            let mut inner = inner_ref.borrow_mut();
            let read_data = self.read_data[inner.next_read_data];
            assert!(
                read_data.len() <= 8,
                "test harness doesn't support multi packet reads"
            );
            buf[..read_data.len()].copy_from_slice(read_data);
            inner.next_read_data += 1;
            Ok(read_data.len())
        }
        fn set_stalled(&self, _ep_addr: EndpointAddress, _stalled: bool) {}
        fn is_stalled(&self, _ep_addr: EndpointAddress) -> bool {
            todo!();
        }
        fn suspend(&self) {
            todo!();
        }
        fn resume(&self) {
            todo!();
        }
        fn poll(&self) -> PollResult {
            let inner_ref = self.inner.lock().unwrap();
            let inner = inner_ref.borrow_mut();
            if inner.write_data.is_empty() {
                assert!(
                    inner.next_read_data < self.read_data.len(),
                    "No data written but all data has been read"
                );

                PollResult::Data {
                    ep_out: 0x0,
                    ep_in_complete: 0x0,
                    ep_setup: 0x1, //setup packet received for ep 0
                }
            } else {
                PollResult::Data {
                    ep_out: 0x0,
                    ep_in_complete: 0x1, //request the next packet
                    ep_setup: 0x0,
                }
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PackedStruct)]
    #[packed_struct(endian = "lsb", bit_numbering = "msb0", size_bytes = "8")]
    struct UsbRequest {
        #[packed_field(bits = "0")]
        direction: bool,
        #[packed_field(bits = "1..=2")]
        request_type: u8,
        #[packed_field(bits = "4..=7")]
        recipient: u8,
        request: u8,
        value: u16,
        index: u16,
        length: u16,
    }

    #[test]
    fn descriptor_ordering_satisfies_boot_spec() {
        #[derive(Default)]
        struct TestHidClass {}

        impl HidConfig for TestHidClass {
            fn report_descriptor(&self) -> &'static [u8] {
                &[]
            }
            fn interface_name(&self) -> &str {
                "Test device"
            }
        }

        let validate_write_data = |v: &Vec<u8>| {
            /*
              Expected descriptor order (https://www.usb.org/sites/default/files/hid1_11.pdf Appendix F.3):
              Configuration descriptor (other Interface, Endpoint, and Vendor Specific descriptors if required)
                Interface descriptor (with Subclass and Protocol specifying Boot Keyboard)
                    Hid descriptor (associated with this Interface)
                    Endpoint descriptor (Hid Interrupt In Endpoint)
                        (other Interface, Endpoint, and Vendor Specific descriptors if required)
            */

            let mut it = v.iter();

            let len = it.next().unwrap();
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
            //todo new test Subclass and Protocol specifying Boot Keyboard
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
        };
        //read a config request for the device config descriptor
        let read_data: &[&[u8]] = &[&UsbRequest {
            direction: UsbDirection::In != UsbDirection::Out,
            request_type: control::RequestType::Standard as u8,
            recipient: control::Recipient::Device as u8,
            request: control::Request::GET_DESCRIPTOR,
            value: (descriptor::descriptor_type::CONFIGURATION as u16) << 8,
            index: 0,
            length: 0xFFFF,
        }
        .pack()
        .unwrap()];

        let usb_bus = TestUsbBus::new(read_data, validate_write_data);

        let usb_alloc = UsbBusAllocator::new(usb_bus);

        let mut hid = UsbHidClass::new(&usb_alloc, TestHidClass::default());

        let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
            .manufacturer("DLKJ")
            .product("Test Hid Device")
            .serial_number("TEST")
            .device_class(USB_CLASS_HID)
            .composite_with_iads()
            .max_packet_size_0(8)
            .build();

        //poll the usb bus
        for _ in 0..10 {
            assert!(usb_dev.poll(&mut [&mut hid]));
        }
    }

    #[test]
    fn get_protocol_default_to_report() {
        #[derive(Default)]
        struct TestHidClass {}

        impl HidConfig for TestHidClass {
            fn report_descriptor(&self) -> &'static [u8] {
                &[]
            }
            fn interface_name(&self) -> &str {
                "Test device"
            }
            fn interface_protocol(&self) -> InterfaceProtocol {
                InterfaceProtocol::Keyboard
            }
            fn interface_sub_class(&self) -> InterfaceSubClass {
                InterfaceSubClass::Boot
            }
        }

        //Get protocol
        let read_data: &[&[u8]] = &[&UsbRequest {
            direction: UsbDirection::In != UsbDirection::Out,
            request_type: control::RequestType::Class as u8,
            recipient: control::Recipient::Interface as u8,
            request: Request::GetProtocol as u8,
            value: 0x0,
            index: 0x0,
            length: 0x1,
        }
        .pack()
        .unwrap()];

        let validate_write_data = |v: &Vec<u8>| {
            assert_eq!(
                v[0],
                HidProtocol::Report as u8,
                "Expected protocol to be Report by default"
            );
        };

        let usb_bus = TestUsbBus::new(read_data, validate_write_data);

        let usb_alloc = UsbBusAllocator::new(usb_bus);

        let mut hid = UsbHidClass::new(&usb_alloc, TestHidClass::default());

        let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
            .manufacturer("DLKJ")
            .product("Test Hid Device")
            .serial_number("TEST")
            .device_class(USB_CLASS_HID)
            .composite_with_iads()
            .max_packet_size_0(8)
            .build();

        //poll the usb bus
        for _ in 0..10 {
            assert!(usb_dev.poll(&mut [&mut hid]));
        }
    }

    #[test]
    fn set_protocol() {
        #[derive(Default)]
        struct TestHidClass {}

        impl HidConfig for TestHidClass {
            fn report_descriptor(&self) -> &'static [u8] {
                &[]
            }
            fn interface_name(&self) -> &str {
                "Test device"
            }
            fn interface_protocol(&self) -> InterfaceProtocol {
                InterfaceProtocol::Keyboard
            }
            fn interface_sub_class(&self) -> InterfaceSubClass {
                InterfaceSubClass::Boot
            }
        }

        let read_data: &[&[u8]] = &[
            //Set protocol to boot
            &UsbRequest {
                direction: UsbDirection::In != UsbDirection::In,
                request_type: control::RequestType::Class as u8,
                recipient: control::Recipient::Interface as u8,
                request: Request::SetProtocol as u8,
                value: HidProtocol::Boot as u16,
                index: 0x0,
                length: 0x0,
            }
            .pack()
            .unwrap(),
            //Get protocol
            &UsbRequest {
                direction: UsbDirection::In != UsbDirection::Out,
                request_type: control::RequestType::Class as u8,
                recipient: control::Recipient::Interface as u8,
                request: Request::GetProtocol as u8,
                value: 0x0,
                index: 0x0,
                length: 0x1,
            }
            .pack()
            .unwrap(),
        ];

        let validate_write_data = |v: &Vec<u8>| {
            assert_eq!(
                v[0],
                HidProtocol::Boot as u8,
                "Expected protocol to be Boot"
            );
        };

        let usb_bus = TestUsbBus::new(read_data, validate_write_data);

        let usb_alloc = UsbBusAllocator::new(usb_bus);

        let mut hid = UsbHidClass::new(&usb_alloc, TestHidClass::default());

        let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
            .manufacturer("DLKJ")
            .product("Test Hid Device")
            .serial_number("TEST")
            .device_class(USB_CLASS_HID)
            .composite_with_iads()
            .max_packet_size_0(8)
            .build();

        //poll the usb bus
        for _ in 0..10 {
            assert!(usb_dev.poll(&mut [&mut hid]));
        }
    }

    #[test]
    fn get_protocol_default_post_reset() {
        #[derive(Default)]
        struct TestHidClass {}

        impl HidConfig for TestHidClass {
            fn report_descriptor(&self) -> &'static [u8] {
                &[]
            }
            fn interface_name(&self) -> &str {
                "Test device"
            }
            fn interface_protocol(&self) -> InterfaceProtocol {
                InterfaceProtocol::Keyboard
            }
            fn interface_sub_class(&self) -> InterfaceSubClass {
                InterfaceSubClass::Boot
            }
        }

        let read_data: &[&[u8]] = &[
            //Set protocol to boot
            &UsbRequest {
                direction: UsbDirection::In != UsbDirection::In,
                request_type: control::RequestType::Class as u8,
                recipient: control::Recipient::Interface as u8,
                request: Request::SetProtocol as u8,
                value: HidProtocol::Boot as u16,
                index: 0x0,
                length: 0x0,
            }
            .pack()
            .unwrap(),
            //Get protocol
            &UsbRequest {
                direction: UsbDirection::In != UsbDirection::Out,
                request_type: control::RequestType::Class as u8,
                recipient: control::Recipient::Interface as u8,
                request: Request::GetProtocol as u8,
                value: 0x0,
                index: 0x0,
                length: 0x1,
            }
            .pack()
            .unwrap(),
        ];

        let validate_write_data = |v: &Vec<u8>| {
            assert_eq!(
                v[0],
                HidProtocol::Report as u8,
                "Expected protocol to be Report post reset"
            );
        };

        let usb_bus = TestUsbBus::new(read_data, validate_write_data);

        let usb_alloc = UsbBusAllocator::new(usb_bus);

        let mut hid = UsbHidClass::new(&usb_alloc, TestHidClass::default());

        let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
            .manufacturer("DLKJ")
            .product("Test Hid Device")
            .serial_number("TEST")
            .device_class(USB_CLASS_HID)
            .composite_with_iads()
            .max_packet_size_0(8)
            .build();

        //poll the usb bus
        assert!(usb_dev.poll(&mut [&mut hid]));

        //simulate a bus reset after setting protocol to boot
        hid.reset();

        for _ in 0..10 {
            assert!(usb_dev.poll(&mut [&mut hid]));
        }
    }
}
