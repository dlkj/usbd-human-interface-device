//!Implements generic UsbHid type representing a USB Human Interface Device

use embedded_time::duration::*;
use log::{error, info, trace};
use usb_device::class_prelude::*;
use usb_device::Result;

const USB_CLASS_HID: u8 = 0x03;
const SPEC_VERSION_1_11: [u8; 2] = [0x11, 0x01]; //1.11 in BCD
const COUNTRY_CODE_NOT_SUPPORTED: u8 = 0x0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, FromPrimitive)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromPrimitive)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromPrimitive)]
#[repr(u8)]
pub enum HidProtocol {
    Boot = 0x00,
    Report = 0x01,
}

pub trait HidClass {
    fn packet_size(&self) -> u8 {
        8
    }
    fn poll_interval(&self) -> Milliseconds {
        Milliseconds(10)
    }
    fn report_descriptor(&self) -> &'static [u8];
    //todo lift interface protocol and sub class up to type-level
    ///only not None if subclass is Boot
    fn interface_protocol(&self) -> InterfaceProtocol {
        InterfaceProtocol::None
    }
    fn interface_sub_class(&self) -> InterfaceSubClass {
        InterfaceSubClass::None
    }
    fn interface_name(&self) -> &str;
    fn set_protocol(&mut self, protocol: HidProtocol) {
        let _ = protocol;
    }
    fn reset(&mut self) {}
}

/// Implements the generic Hid Device
pub struct UsbHid<'a, B: UsbBus, C: HidClass> {
    hid_class: C,
    interface_number: InterfaceNumber,
    out_endpoint: EndpointOut<'a, B>,
    in_endpoint: EndpointIn<'a, B>,
    string_index: StringIndex,
    protocol: HidProtocol,
}

impl<B: UsbBus, C: HidClass> UsbHid<'_, B, C> {
    pub fn new(usb_alloc: &'_ UsbBusAllocator<B>, hid_class: C) -> UsbHid<'_, B, C> {
        let interval_ms = hid_class.poll_interval().integer() as u8;
        let interface_number = usb_alloc.interface();

        info!("Assigned interface {:X}", u8::from(interface_number));

        UsbHid {
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
            Ok([
                SPEC_VERSION_1_11[0], //Hid Class spec version 1.10 (BCD)
                SPEC_VERSION_1_11[1],
                COUNTRY_CODE_NOT_SUPPORTED, //Country code 0 - not supported
                1,                          //Num descriptors
                DescriptorType::Report as u8, //Class descriptor type is report
                (descriptor_len & 0xFF) as u8, //Descriptor length
                (descriptor_len >> 8 & 0xFF) as u8,
            ])
        }
    }

    pub fn protocol(&self) -> HidProtocol {
        self.protocol
    }

    fn set_protocol(&mut self, protocol: HidProtocol) {
        self.protocol = protocol;
        self.hid_class.set_protocol(protocol);
    }

    pub fn write_report(&self, data: &[u8]) -> Result<usize> {
        self.in_endpoint.write(data)
    }

    pub fn read_report(&self, data: &mut [u8]) -> Result<usize> {
        self.out_endpoint.read(data)
    }

    fn control_in_get_descriptors(&mut self, transfer: ControlIn<B>) {
        let request: &control::Request = transfer.request();
        match num::FromPrimitive::from_u16(request.value >> 8) {
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

impl<B: UsbBus, C: HidClass> UsbClass<B> for UsbHid<'_, B, C> {
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
                match num::FromPrimitive::from_u8(request.request) {
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

        match num::FromPrimitive::from_u8(request.request) {
            Some(Request::SetIdle) => {
                transfer.accept().ok(); //Not supported
            }
            Some(Request::SetReport) => {
                // Not supported - data reports handled via interrupt endpoints
                transfer.reject().ok();
            }
            Some(Request::SetProtocol) => {
                if let Some(protocol) = num::FromPrimitive::from_u16(request.value) {
                    self.set_protocol(protocol);
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
        self.set_protocol(HidProtocol::Report);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hid::HidClass;
    use std::cell::RefCell;
    use std::sync::Mutex;
    use std::vec::Vec;
    use usb_device::bus::*;
    use usb_device::prelude::*;
    use usb_device::*;

    struct TestUsbBus<F> {
        next_ep_index: usize,
        read_data: &'static [&'static [u8]],
        write_val: F,
        inner: Mutex<RefCell<TestUsbBusInner>>,
    }
    struct TestUsbBusInner {
        next_read_data: usize,
        write_data: Vec<u8>,
    }

    impl<F> TestUsbBus<F> {
        fn new(read_data: &'static [&'static [u8]], write_val: F) -> Self {
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

    impl<F> UsbBus for TestUsbBus<F>
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

    #[test]
    fn descriptor_ordering_satisfies_boot_spec() {
        #[derive(Default)]
        struct TestHidClass {}

        impl HidClass for TestHidClass {
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

        //todo change to "packed_struct"
        //read a config request for the device config descriptor
        let read_data: &[&[u8]] = &[&[
            //direction | request type| recipient
            UsbDirection::In as u8
                | (control::RequestType::Standard as u8) << 5
                | control::Recipient::Device as u8,
            control::Request::GET_DESCRIPTOR, //request
            0,                                //value[0]
            descriptor::descriptor_type::CONFIGURATION as u8, //value[1]
            0,                                //index[0]
            0x00,                             //index[1]
            0xFF,                             //length[0]
            0xFF,                             //length[1]
        ]];

        let usb_bus = TestUsbBus::new(read_data, validate_write_data);

        let usb_alloc = UsbBusAllocator::new(usb_bus);

        let mut hid = UsbHid::new(&usb_alloc, TestHidClass::default());

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

        impl HidClass for TestHidClass {
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

        //todo change to "packed_struct"
        //Get protocol
        let read_data: &[&[u8]] = &[&[
            //direction | request type| recipient
            UsbDirection::In as u8
                | (control::RequestType::Class as u8) << 5
                | control::Recipient::Interface as u8,
            Request::GetProtocol as u8, //request
            0x00,                       //value[0]
            0x00,                       //value[1]
            0x00,                       //index[0]
            0x00,                       //index[1]
            0x01,                       //length[0]
            0x00,                       //length[1]
        ]];

        let validate_write_data = |v: &Vec<u8>| {
            assert_eq!(
                v[0],
                HidProtocol::Report as u8,
                "Expected protocol to be Report by default"
            );
        };

        let usb_bus = TestUsbBus::new(read_data, validate_write_data);

        let usb_alloc = UsbBusAllocator::new(usb_bus);

        let mut hid = UsbHid::new(&usb_alloc, TestHidClass::default());

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

        impl HidClass for TestHidClass {
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

        //todo change to "packed_struct"
        let read_data: &[&[u8]] = &[
            //Set protocol to boot
            &[
                //direction | request type| recipient
                UsbDirection::Out as u8
                    | (control::RequestType::Class as u8) << 5
                    | control::Recipient::Interface as u8,
                Request::SetProtocol as u8, //request
                0x00,                       //value[0]
                0x00,                       //value[1]
                0x00,                       //index[0]
                0x00,                       //index[1]
                0x00,                       //length[0]
                0x00,                       //length[1]
            ],
            //Get protocol
            &[
                //direction | request type| recipient
                UsbDirection::In as u8
                    | (control::RequestType::Class as u8) << 5
                    | control::Recipient::Interface as u8,
                Request::GetProtocol as u8, //request
                0x00,                       //value[0]
                0x00,                       //value[1]
                0x00,                       //index[0]
                0x00,                       //index[1]
                0x01,                       //length[0]
                0x00,                       //length[1]
            ],
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

        let mut hid = UsbHid::new(&usb_alloc, TestHidClass::default());

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

        impl HidClass for TestHidClass {
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

        //todo change to "packed_struct"
        let read_data: &[&[u8]] = &[
            //Set protocol to boot
            &[
                //direction | request type| recipient
                UsbDirection::Out as u8
                    | (control::RequestType::Class as u8) << 5
                    | control::Recipient::Interface as u8,
                Request::SetProtocol as u8, //request
                0x00,                       //value[0]
                0x00,                       //value[1]
                0x00,                       //index[0]
                0x00,                       //index[1]
                0x00,                       //length[0]
                0x00,                       //length[1]
            ],
            //Get protocol
            &[
                //direction | request type| recipient
                UsbDirection::In as u8
                    | (control::RequestType::Class as u8) << 5
                    | control::Recipient::Interface as u8,
                Request::GetProtocol as u8, //request
                0x00,                       //value[0]
                0x00,                       //value[1]
                0x00,                       //index[0]
                0x00,                       //index[1]
                0x01,                       //length[0]
                0x00,                       //length[1]
            ],
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

        let mut hid = UsbHid::new(&usb_alloc, TestHidClass::default());

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
