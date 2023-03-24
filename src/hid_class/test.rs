#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::cell::RefCell;
use std::sync::Mutex;
use std::vec::Vec;

use crate::hid_class::descriptor::USB_CLASS_HID;
use crate::interface::raw::RawInterfaceBuilder;
use env_logger::Env;
use fugit::MillisDurationU32;
use packed_struct::prelude::*;
use usb_device::bus::PollResult;
use usb_device::prelude::*;
use usb_device::UsbDirection;

use super::*;

fn init_logging() {
    let _ = env_logger::Builder::from_env(Env::default().default_filter_or("trace"))
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
        .add_interface(RawInterfaceBuilder::<64, 64>::new(&[]).unwrap().build())
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
        .add_interface(RawInterfaceBuilder::<64, 64>::new(&[]).unwrap().build())
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
        .add_interface(RawInterfaceBuilder::<64, 64>::new(&[]).unwrap().build())
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
        .add_interface(RawInterfaceBuilder::<64, 64>::new(&[]).unwrap().build())
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
        .add_interface(
            RawInterfaceBuilder::<64, 64>::new(&[])
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
        .add_interface(
            RawInterfaceBuilder::<64, 64>::new(&[])
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
        .add_interface(
            RawInterfaceBuilder::<64, 64>::new(&[])
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
        .add_interface(
            RawInterfaceBuilder::<64, 64>::new(&[])
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
        .add_interface(
            RawInterfaceBuilder::<64, 64>::new(&[])
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
fn get_report_idle_default_post_reset() {
    const REPORT_ID: u8 = 0x4;
    const IDLE_DEFAULT: MillisDurationU32 = MillisDurationU32::millis(40);
    const IDLE_NEW: MillisDurationU32 = MillisDurationU32::millis(88);

    init_logging();

    let manager = UsbTestManager::default();

    let usb_alloc = UsbBusAllocator::new(TestUsbBus::new(&manager));

    let mut hid = UsbHidClassBuilder::new()
        .add_interface(
            RawInterfaceBuilder::<64, 64>::new(&[])
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
