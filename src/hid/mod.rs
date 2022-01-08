use usb_device::class_prelude::*;

pub struct HIDDevice<'a, B: UsbBus> {
    usb_bus: &'a B,
}

impl<'a, B: UsbBus> HIDDevice<'a, B> {}
