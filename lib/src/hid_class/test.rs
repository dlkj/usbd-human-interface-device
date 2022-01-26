use std::cell::RefCell;
use std::sync::Mutex;
use std::vec::Vec;

use crate::hid_class::prelude::UsbHidInterfaceBuilder;
use embedded_time::duration::Milliseconds;
use embedded_time::fixed_point::FixedPoint;
use env_logger::Env;
use usb_device::bus::PollResult;
use usb_device::prelude::*;
use usb_device::UsbDirection;

use super::*;

fn init_logging() {
    let _ = env_logger::Builder::from_env(Env::default().default_filter_or("trace"))
        .is_test(true)
        .try_init();
}

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
        todo!()
    }
    fn set_device_address(&self, _addr: u8) {
        todo!()
    }
    fn write(&self, _ep_addr: EndpointAddress, buf: &[u8]) -> Result<usize> {
        let inner_ref = self.inner.lock().unwrap();
        let mut inner = inner_ref.borrow_mut();

        inner.write_data.extend_from_slice(buf);

        if buf.len() < 8 && inner.next_read_data >= self.read_data.len() {
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
        todo!()
    }
    fn suspend(&self) {
        todo!()
    }
    fn resume(&self) {
        todo!()
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

    let validate_write_data = |v: &Vec<u8>| {
        /*
          Expected descriptor order (<https://www.usb.org/sites/default/files/hid1_11.pdf> Appendix F.3):
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
        request_type: RequestType::Standard as u8,
        recipient: Recipient::Device as u8,
        request: Request::GET_DESCRIPTOR,
        value: (usb_device::descriptor::descriptor_type::CONFIGURATION as u16) << 8,
        index: 0,
        length: 0xFFFF,
    }
    .pack()
    .unwrap()];

    let usb_bus = TestUsbBus::new(read_data, validate_write_data);

    let usb_alloc = UsbBusAllocator::new(usb_bus);

    let mut hid = UsbHidClassBuilder::new(&usb_alloc)
        .new_interface(UsbHidInterfaceBuilder::new(&[]).build_interface())
        .unwrap()
        .build()
        .unwrap();

    let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
        .manufacturer("usbd-hid-devices")
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
    init_logging();

    //Get protocol
    let read_data: &[&[u8]] = &[&UsbRequest {
        direction: UsbDirection::In != UsbDirection::Out,
        request_type: RequestType::Class as u8,
        recipient: Recipient::Interface as u8,
        request: HidRequest::GetProtocol as u8,
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

    let mut hid = UsbHidClassBuilder::new(&usb_alloc)
        .new_interface(UsbHidInterfaceBuilder::new(&[]).build_interface())
        .unwrap()
        .build()
        .unwrap();

    let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
        .manufacturer("usbd-hid-devices")
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
    init_logging();

    let read_data: &[&[u8]] = &[
        //Set protocol to boot
        &UsbRequest {
            direction: UsbDirection::In != UsbDirection::In,
            request_type: RequestType::Class as u8,
            recipient: Recipient::Interface as u8,
            request: HidRequest::SetProtocol as u8,
            value: HidProtocol::Boot as u16,
            index: 0x0,
            length: 0x0,
        }
        .pack()
        .unwrap(),
        //Get protocol
        &UsbRequest {
            direction: UsbDirection::In != UsbDirection::Out,
            request_type: RequestType::Class as u8,
            recipient: Recipient::Interface as u8,
            request: HidRequest::GetProtocol as u8,
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

    let mut hid = UsbHidClassBuilder::new(&usb_alloc)
        .new_interface(UsbHidInterfaceBuilder::new(&[]).build_interface())
        .unwrap()
        .build()
        .unwrap();

    let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
        .manufacturer("usbd-hid-devices")
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
    init_logging();

    let read_data: &[&[u8]] = &[
        //Set protocol to boot
        &UsbRequest {
            direction: UsbDirection::In != UsbDirection::In,
            request_type: RequestType::Class as u8,
            recipient: Recipient::Interface as u8,
            request: HidRequest::SetProtocol as u8,
            value: HidProtocol::Boot as u16,
            index: 0x0,
            length: 0x0,
        }
        .pack()
        .unwrap(),
        //Get protocol
        &UsbRequest {
            direction: UsbDirection::In != UsbDirection::Out,
            request_type: RequestType::Class as u8,
            recipient: Recipient::Interface as u8,
            request: HidRequest::GetProtocol as u8,
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

    let mut hid = UsbHidClassBuilder::new(&usb_alloc)
        .new_interface(UsbHidInterfaceBuilder::new(&[]).build_interface())
        .unwrap()
        .build()
        .unwrap();

    let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
        .manufacturer("usbd-hid-devices")
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

#[test]
fn get_global_idle_default() {
    init_logging();

    const IDLE_DEFAULT: Milliseconds = Milliseconds(40);

    //Get idle
    let read_data: &[&[u8]] = &[&UsbRequest {
        direction: UsbDirection::In != UsbDirection::Out,
        request_type: RequestType::Class as u8,
        recipient: Recipient::Interface as u8,
        request: HidRequest::GetIdle as u8,
        value: 0x0,
        index: 0x0,
        length: 0x1,
    }
    .pack()
    .unwrap()];

    let validate_write_data = |v: &Vec<u8>| {
        assert_eq!(
            Milliseconds(v[0] as u32 * 4),
            IDLE_DEFAULT,
            "Unexpected idle value"
        );
    };

    let usb_bus = TestUsbBus::new(read_data, validate_write_data);

    let usb_alloc = UsbBusAllocator::new(usb_bus);

    let mut hid = UsbHidClassBuilder::new(&usb_alloc)
        .new_interface(
            UsbHidInterfaceBuilder::new(&[])
                .idle_default(IDLE_DEFAULT)
                .unwrap()
                .build_interface(),
        )
        .unwrap()
        .build()
        .unwrap();

    let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
        .manufacturer("usbd-hid-devices")
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
fn set_global_idle() {
    init_logging();
    const IDLE_DEFAULT: Milliseconds = Milliseconds(40);
    const IDLE_NEW: Milliseconds = Milliseconds(88);

    let read_data: &[&[u8]] = &[
        //Set idle
        &UsbRequest {
            direction: UsbDirection::In != UsbDirection::In,
            request_type: RequestType::Class as u8,
            recipient: Recipient::Interface as u8,
            request: HidRequest::SetIdle as u8,
            value: (IDLE_NEW.integer() as u16 / 4) << 8,
            index: 0x0,
            length: 0x0,
        }
        .pack()
        .unwrap(),
        //Get idle
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
    ];

    let validate_write_data = |v: &Vec<u8>| {
        assert_eq!(
            Milliseconds(v[0] as u32 * 4),
            IDLE_NEW,
            "Unexpected idle value"
        );
    };

    let usb_bus = TestUsbBus::new(read_data, validate_write_data);

    let usb_alloc = UsbBusAllocator::new(usb_bus);

    let mut hid = UsbHidClassBuilder::new(&usb_alloc)
        .new_interface(
            UsbHidInterfaceBuilder::new(&[])
                .idle_default(IDLE_DEFAULT)
                .unwrap()
                .build_interface(),
        )
        .unwrap()
        .build()
        .unwrap();

    let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
        .manufacturer("usbd-hid-devices")
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
fn get_global_idle_default_post_reset() {
    init_logging();

    const IDLE_DEFAULT: Milliseconds = Milliseconds(40);
    const IDLE_NEW: Milliseconds = Milliseconds(88);

    let read_data: &[&[u8]] = &[
        //Set global idle
        &UsbRequest {
            direction: UsbDirection::In != UsbDirection::In,
            request_type: RequestType::Class as u8,
            recipient: Recipient::Interface as u8,
            request: HidRequest::SetIdle as u8,
            value: (IDLE_NEW.integer() as u16 / 4) << 8,
            index: 0x0,
            length: 0x0,
        }
        .pack()
        .unwrap(),
        //Get global idle
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
    ];

    let validate_write_data = |v: &Vec<u8>| {
        assert_eq!(
            Milliseconds(v[0] as u32 * 4),
            IDLE_DEFAULT,
            "Unexpected idle value"
        );
    };

    let usb_bus = TestUsbBus::new(read_data, validate_write_data);

    let usb_alloc = UsbBusAllocator::new(usb_bus);

    let mut hid = UsbHidClassBuilder::new(&usb_alloc)
        .new_interface(
            UsbHidInterfaceBuilder::new(&[])
                .idle_default(IDLE_DEFAULT)
                .unwrap()
                .build_interface(),
        )
        .unwrap()
        .build()
        .unwrap();

    let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
        .manufacturer("usbd-hid-devices")
        .product("Test Hid Device")
        .serial_number("TEST")
        .device_class(USB_CLASS_HID)
        .composite_with_iads()
        .max_packet_size_0(8)
        .build();

    //poll the usb bus
    assert!(usb_dev.poll(&mut [&mut hid]));

    //simulate a bus reset after setting idle value
    hid.reset();

    for _ in 0..10 {
        assert!(usb_dev.poll(&mut [&mut hid]));
    }
}

#[test]
fn get_report_idle_default() {
    init_logging();

    const IDLE_DEFAULT: Milliseconds = Milliseconds(40);
    const REPORT_ID: u8 = 0xAB;

    //Get idle
    let read_data: &[&[u8]] = &[&UsbRequest {
        direction: UsbDirection::In != UsbDirection::Out,
        request_type: RequestType::Class as u8,
        recipient: Recipient::Interface as u8,
        request: HidRequest::GetIdle as u8,
        value: REPORT_ID as u16,
        index: 0x0,
        length: 0x1,
    }
    .pack()
    .unwrap()];

    let validate_write_data = |v: &Vec<u8>| {
        assert_eq!(
            Milliseconds(v[0] as u32 * 4),
            IDLE_DEFAULT,
            "Unexpected idle value"
        );
    };

    let usb_bus = TestUsbBus::new(read_data, validate_write_data);

    let usb_alloc = UsbBusAllocator::new(usb_bus);

    let mut hid = UsbHidClassBuilder::new(&usb_alloc)
        .new_interface(
            UsbHidInterfaceBuilder::new(&[])
                .idle_default(IDLE_DEFAULT)
                .unwrap()
                .build_interface(),
        )
        .unwrap()
        .build()
        .unwrap();

    let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
        .manufacturer("usbd-hid-devices")
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
fn set_report_idle() {
    init_logging();
    const IDLE_DEFAULT: Milliseconds = Milliseconds(40);
    const IDLE_NEW: Milliseconds = Milliseconds(88);
    const REPORT_ID: u8 = 0xAB;

    let read_data: &[&[u8]] = &[
        //Set report idle
        &UsbRequest {
            direction: UsbDirection::In != UsbDirection::In,
            request_type: RequestType::Class as u8,
            recipient: Recipient::Interface as u8,
            request: HidRequest::SetIdle as u8,
            value: (IDLE_NEW.integer() as u16 / 4) << 8 | REPORT_ID as u16,
            index: 0x0,
            length: 0x0,
        }
        .pack()
        .unwrap(),
        //Get report idle
        &UsbRequest {
            direction: UsbDirection::In != UsbDirection::Out,
            request_type: RequestType::Class as u8,
            recipient: Recipient::Interface as u8,
            request: HidRequest::GetIdle as u8,
            value: REPORT_ID as u16,
            index: 0x0,
            length: 0x1,
        }
        .pack()
        .unwrap(),
        //Get global idle
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
    ];

    let validate_write_data = |v: &Vec<u8>| {
        assert_eq!(
            Milliseconds(v[0] as u32 * 4),
            IDLE_NEW,
            "Unexpected report idle value"
        );
        assert_eq!(
            Milliseconds(v[1] as u32 * 4),
            IDLE_DEFAULT,
            "Unexpected global idle value"
        );
    };

    let usb_bus = TestUsbBus::new(read_data, validate_write_data);

    let usb_alloc = UsbBusAllocator::new(usb_bus);

    let mut hid = UsbHidClassBuilder::new(&usb_alloc)
        .new_interface(
            UsbHidInterfaceBuilder::new(&[])
                .idle_default(IDLE_DEFAULT)
                .unwrap()
                .build_interface(),
        )
        .unwrap()
        .build()
        .unwrap();

    let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
        .manufacturer("usbd-hid-devices")
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
fn get_report_idle_default_post_reset() {
    const REPORT_ID: u8 = 0xAB;

    init_logging();

    const IDLE_DEFAULT: Milliseconds = Milliseconds(40);
    const IDLE_NEW: Milliseconds = Milliseconds(88);

    let read_data: &[&[u8]] = &[
        //Set report idle
        &UsbRequest {
            direction: UsbDirection::In != UsbDirection::In,
            request_type: RequestType::Class as u8,
            recipient: Recipient::Interface as u8,
            request: HidRequest::SetIdle as u8,
            value: (IDLE_NEW.integer() as u16 / 4) << 8 | REPORT_ID as u16,
            index: 0x0,
            length: 0x0,
        }
        .pack()
        .unwrap(),
        //Get report idle
        &UsbRequest {
            direction: UsbDirection::In != UsbDirection::Out,
            request_type: RequestType::Class as u8,
            recipient: Recipient::Interface as u8,
            request: HidRequest::GetIdle as u8,
            value: REPORT_ID as u16,
            index: 0x0,
            length: 0x1,
        }
        .pack()
        .unwrap(),
    ];

    let validate_write_data = |v: &Vec<u8>| {
        assert_eq!(
            Milliseconds(v[0] as u32 * 4),
            IDLE_DEFAULT,
            "Unexpected idle value"
        );
    };

    let usb_bus = TestUsbBus::new(read_data, validate_write_data);

    let usb_alloc = UsbBusAllocator::new(usb_bus);

    let mut hid = UsbHidClassBuilder::new(&usb_alloc)
        .new_interface(
            UsbHidInterfaceBuilder::new(&[])
                .idle_default(IDLE_DEFAULT)
                .unwrap()
                .build_interface(),
        )
        .unwrap()
        .build()
        .unwrap();

    let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
        .manufacturer("usbd-hid-devices")
        .product("Test Hid Device")
        .serial_number("TEST")
        .device_class(USB_CLASS_HID)
        .composite_with_iads()
        .max_packet_size_0(8)
        .build();

    //poll the usb bus
    assert!(usb_dev.poll(&mut [&mut hid]));

    //simulate a bus reset after setting idle value
    hid.reset();

    for _ in 0..10 {
        assert!(usb_dev.poll(&mut [&mut hid]));
    }
}
