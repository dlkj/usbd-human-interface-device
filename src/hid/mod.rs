//!Implements generic HID type

use embedded_time::duration::*;
use log::{debug, error, info, trace};
use usb_device::class_prelude::*;
use usb_device::Result;

const USB_CLASS_HID: u8 = 0x03;
const HID_CLASS_SPEC_1_11: [u8; 2] = [0x11, 0x01]; //1.11 in BCD
const HID_COUNTRY_CODE_NOT_SUPPORTED: u8 = 0x0;

const HID_REQ_SET_IDLE: u8 = 0x0a;
const HID_REQ_GET_IDLE: u8 = 0x02;
const HID_REQ_GET_REPORT: u8 = 0x01;
const HID_REQ_SET_REPORT: u8 = 0x09;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum HIDProtocol {
    None = 0x00,
    Keyboard = 0x01,
    _Mouse = 0x02,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HIDDescriptorType {
    Hid = 0x21,
    Report = 0x22,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum HIDSubclass {
    None = 0x00,
    Boot = 0x01,
}

pub trait HIDClass {
    fn packet_size(&self) -> u8;
    fn poll_interval(&self) -> Milliseconds;
    fn report_descriptor(&self) -> &'static [u8];
}

/// Implements the generic HID Device
pub struct HID<'a, B: UsbBus, C: HIDClass> {
    hid_class: C,
    interface_number: InterfaceNumber,
    out_endpoint: EndpointOut<'a, B>,
    in_endpoint: EndpointIn<'a, B>,
    string_index: StringIndex,
}

impl<B: UsbBus, C: HIDClass> HID<'_, B, C> {
    pub fn new(usb_alloc: &'_ UsbBusAllocator<B>, hid_class: C) -> HID<'_, B, C> {
        let interval_ms = hid_class.poll_interval().integer() as u8;
        let interface_number = usb_alloc.interface();

        info!("Assigned interface {:X}", u8::from(interface_number));

        HID {
            hid_class,
            interface_number,
            //Report size must be 8 bytes or less, so also capping the packet size
            out_endpoint: usb_alloc.interrupt(8, interval_ms),
            in_endpoint: usb_alloc.interrupt(8, interval_ms),
            string_index: usb_alloc.string(),
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
                HID_CLASS_SPEC_1_11[0], //HID Class spec version 1.10 (BCD)
                HID_CLASS_SPEC_1_11[1],
                HID_COUNTRY_CODE_NOT_SUPPORTED, //Country code 0 - not supported
                1,                              //Num descriptors - 1
                HIDDescriptorType::Report as u8, //Class descriptor type is report
                (descriptor_len & 0xFF) as u8,  //Descriptor length
                (descriptor_len >> 8 & 0xFF) as u8,
            ])
        }
    }

    pub fn interface_number(&self) -> InterfaceNumber {
        self.interface_number
    }

    pub fn write_report(&self, data: &[u8]) -> Result<usize> {
        self.in_endpoint.write(data)
    }

    pub fn read_report(&self, data: &mut [u8]) -> Result<usize> {
        self.out_endpoint.read(data)
    }
}

impl<B: UsbBus, C: HIDClass> UsbClass<B> for HID<'_, B, C> {
    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> Result<()> {
        trace!(
            "Getting config descriptor for interface {:X}",
            u8::from(self.interface_number)
        );

        writer.interface_alt(
            self.interface_number,
            usb_device::device::DEFAULT_ALTERNATE_SETTING,
            USB_CLASS_HID,
            HIDSubclass::Boot as u8,
            HIDProtocol::Keyboard as u8,
            Some(self.string_index),
        )?;

        //HID descriptor
        writer.write(HIDDescriptorType::Hid as u8, &self.hid_descriptor()?)?;

        //Endpoint descriptors
        writer.endpoint(&self.out_endpoint)?;
        writer.endpoint(&self.in_endpoint)?;

        Ok(())
    }

    fn get_string(&self, index: StringIndex, _lang_id: u16) -> Option<&str> {
        if index == self.string_index {
            Some("Keyboard")
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
            "ctrl_in: interface {:X} - request type: {:?}, request: {:X}, value: {:X}",
            u8::from(self.interface_number),
            request.request_type,
            request.request,
            request.value
        );

        match (request.request_type, request.request) {
            (control::RequestType::Standard, control::Request::GET_DESCRIPTOR) => {
                let request_value = (request.value >> 8) as u8;

                if request_value == HIDDescriptorType::Report as u8 {
                    debug!(
                        "interface {:X} - get report descriptor",
                        u8::from(self.interface_number)
                    );

                    transfer
                        .accept_with_static(self.hid_class.report_descriptor())
                        .unwrap_or_else(|e| {
                            error!(
                                "interface {:X} - Failed to send report descriptor - {:?}",
                                u8::from(self.interface_number),
                                e
                            )
                        });
                } else if request_value == HIDDescriptorType::Hid as u8 {
                    debug!(
                        "interface {:X} - get hid descriptor",
                        u8::from(self.interface_number)
                    );
                    match self.hid_descriptor() {
                        Err(e) => {
                            error!(
                                "interface {:X} - Failed to generate HID descriptor - {:?}",
                                u8::from(self.interface_number),
                                e
                            );
                            transfer.reject().ok();
                        }
                        Ok(hid_descriptor) => {
                            let mut buffer = [0; 9];
                            buffer[0] = buffer.len() as u8;
                            buffer[1] = HIDDescriptorType::Hid as u8;
                            (&mut buffer[2..]).copy_from_slice(&hid_descriptor);
                            transfer.accept_with(&buffer).unwrap_or_else(|e| {
                                error!(
                                    "interface {:X} - Failed to send HID descriptor - {:?}",
                                    u8::from(self.interface_number),
                                    e
                                );
                            });
                        }
                    }
                } else {
                    trace!(
                        "interface {:X} - unsupported - request type: {:?}, request: {:X}, value: {:X}",
                        u8::from(self.interface_number),
                        request.request_type,
                        request.request,
                        request.value
                    );
                }
            }
            (control::RequestType::Standard, _) => {
                // We shouldn't handle any other standard requests, leave for
                // [`UsbDevice`](crate::device::UsbDevice) to handle
            }
            (control::RequestType::Class, HID_REQ_GET_REPORT) => {
                trace!(
                    "interface {:X} - unsupported - request type: {:?}, request: {:X}, value: {:X}",
                    u8::from(self.interface_number),
                    request.request_type,
                    request.request,
                    request.value
                );

                // Not supported - data reports handled via interrupt endpoints
                transfer.reject().ok(); // Not supported
            }
            (control::RequestType::Class, HID_REQ_GET_IDLE) => {
                trace!(
                    "interface {:X} - unsupported - request type: {:?}, request: {:X}, value: {:X}",
                    u8::from(self.interface_number),
                    request.request_type,
                    request.request,
                    request.value
                );
                transfer.reject().ok(); // Not supported
            }
            _ => {
                trace!(
                    "interface {:X} - unsupported - request type: {:?}, request: {:X}, value: {:X}",
                    u8::from(self.interface_number),
                    request.request_type,
                    request.request,
                    request.value
                );
                transfer.reject().ok(); // Not supported
            }
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
            "ctrl_out: interface {:X} - request type: {:?}, request: {:X}, value: {:X}",
            u8::from(self.interface_number),
            request.request_type,
            request.request,
            request.value
        );

        match request.request {
            HID_REQ_SET_IDLE => {
                trace!(
                    "interface {:X} - unsupported - request type: {:?}, request: {:X}, value: {:X}",
                    u8::from(self.interface_number),
                    request.request_type,
                    request.request,
                    request.value
                );
                transfer.accept().ok(); //Not supported
            }
            HID_REQ_SET_REPORT => {
                trace!(
                    "interface {:X} - unsupported - request type: {:?}, request: {:X}, value: {:X}",
                    u8::from(self.interface_number),
                    request.request_type,
                    request.request,
                    request.value
                );

                // Not supported - data reports handled via interrupt endpoints
                transfer.reject().ok();
            }
            _ => {
                trace!(
                    "interface {:X} - unsupported - request type: {:?}, request: {:X}, value: {:X}",
                    u8::from(self.interface_number),
                    request.request_type,
                    request.request,
                    request.value
                );
                transfer.reject().ok();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::hid::HIDClass;
    use embedded_time::duration::Milliseconds;
    use std::cell::RefCell;
    use std::sync::Mutex;
    use std::vec::Vec;
    use usb_device::bus::*;
    use usb_device::class_prelude::*;
    use usb_device::prelude::*;
    use usb_device::*;

    struct TestUsbBus<F> {
        next_ep_index: usize,
        write_data: Mutex<RefCell<Vec<u8>>>,
        write_val: F,
    }

    impl<F> TestUsbBus<F> {
        fn new(write_val: F) -> Self {
            TestUsbBus {
                next_ep_index: 0,
                write_data: Mutex::new(RefCell::new(Vec::new())),
                write_val: write_val,
            }
        }
    }

    impl<F> UsbBus for TestUsbBus<F>
    where
        F: core::marker::Sync + Fn(&Vec<u8>) -> (),
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
            self.write_data
                .lock()
                .unwrap()
                .borrow_mut()
                .extend_from_slice(buf);

            if buf.len() < 8 {
                //if we get less than a full buffer, the write is complete, validate the buffer
                (self.write_val)(&self.write_data.lock().unwrap().borrow());
            }

            Ok(buf.len())
        }
        fn read(&self, _ep_addr: EndpointAddress, buf: &mut [u8]) -> Result<usize> {
            //todo change to "packed_struct"

            //read a config request for the device config descriptor
            //direction | request type| recipient
            buf[0] = UsbDirection::In as u8
                | (control::RequestType::Standard as u8) << 5
                | control::Recipient::Device as u8;
            buf[1] = control::Request::GET_DESCRIPTOR; //request
            buf[2] = 0; //value[0]
            buf[3] = descriptor::descriptor_type::CONFIGURATION as u8; //value[1]
            buf[4] = 0; //index[0]
            buf[5] = 0x00; //index[1]
            buf[6] = 0xFF; //length[0]
            buf[7] = 0xFF; //length[1]

            Ok(8)
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
            if self.write_data.lock().unwrap().borrow().is_empty() {
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

        impl HIDClass for TestHidClass {
            fn packet_size(&self) -> u8 {
                8
            }
            fn poll_interval(&self) -> embedded_time::duration::Milliseconds {
                Milliseconds(10)
            }
            fn report_descriptor(&self) -> &'static [u8] {
                &[]
            }
        }

        let validate_write_data = |v: &Vec<u8>| {
            /*
              Expected descriptor order (https://www.usb.org/sites/default/files/hid1_11.pdf Appendix F.3):
              Configuration descriptor (other Interface, Endpoint, and Vendor Specific descriptors if required)
                Interface descriptor (with Subclass and Protocol specifying Boot Keyboard)
                    HID descriptor (associated with this Interface)
                    Endpoint descriptor (HID Interrupt In Endpoint)
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
            assert_eq!(*(it.next().unwrap()), 0x21, "Expected HID descriptor");
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

        let usb_bus = TestUsbBus::new(validate_write_data);

        let usb_alloc = UsbBusAllocator::new(usb_bus);

        let mut hid = super::HID::new(&usb_alloc, TestHidClass::default());

        let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
            .manufacturer("DLKJ")
            .product("Test Hid Device")
            .serial_number("TEST")
            .device_class(3) // HID - from: https://www.usb.org/defined-class-codes
            .composite_with_iads()
            .max_packet_size_0(8)
            .build();

        //poll the usb bus
        for _ in 0..10 {
            assert!(usb_dev.poll(&mut [&mut hid]));
        }
    }
}
