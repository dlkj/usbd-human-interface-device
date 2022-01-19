//!HID consumer control devices

use crate::hid_class::prelude::*;
use embedded_time::duration::Milliseconds;
use log::warn;
use usb_device::class_prelude::*;

///Consumer control report descriptor - Single `u16` consumer control usage code (2 bytes)
#[rustfmt::skip]
pub const SINGLE_CODE_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x0C, // Usage Page (Consumer),
    0x09, 0x01, // Usage (Consumer Control),
    0xA1, 0x01, // Collection (Application),
    0x75, 0x10, //     Report Size(16)
    0x95, 0x01, //     Report Count(1)
    0x15, 0x00, //     Logical Minium(0)
    0x26, 0x9C, 0x02, //     Logical Maximum(0x029C)
    0x19, 0x00, //     Usage Minium(0)
    0x2A, 0x9C, 0x02, //     Usage Maximum(0x029C)
    0x81, 0x00, //     Input (Array, Data, Variable)
    0xC0, // End Collection
];

///Consumer control report descriptor - Four `u16` consumer control usage codes as an array (8 bytes)
#[rustfmt::skip]
pub const MULTIPLE_CODE_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x0C, // Usage Page (Consumer),
    0x09, 0x01, // Usage (Consumer Control),
    0xA1, 0x01, // Collection (Application),
    0x75, 0x10, //     Report Size(16)
    0x95, 0x04, //     Report Count(4)
    0x15, 0x00, //     Logical Minium(0)
    0x26, 0x9C, 0x02, //     Logical Maximum(0x029C)
    0x19, 0x00, //     Usage Minium(0)
    0x2A, 0x9C, 0x02, //     Usage Maximum(0x029C)
    0x81, 0x00, //     Input (Array, Data, Variable)
    0xC0, // End Collection
];

// #[rustfmt::skip]
// pub const HID_CONSUMER_CONTROL_VOL_KNOB_7_ARRAY_REPORT_DESCRIPTOR: &[u8] = &[
//     0x05, 0x0C, // Usage Page (Consumer),
//     0x09, 0x01, // Usage (Consumer Control),
//     0xA1, 0x01, // Collection (Application),
//                 //
//     0x09, 0xE0, //     Usage (Volume)
//     0x15, 0x9C, //     Logical Minimum(-100)
//     0x25, 0x64, //     Logical Maximum(100)
//     0x95, 0x01, //     ReportCount(1),
//     0x75, 0x08, //     ReportSize(8),
//     0x81, 0x06, //     Input (Data, Variable, Relative)
//                 //
//     0x75, 0x08, //     Report Size(8)
//     0x95, 0x07, //     Report Count(7)
//     0x15, 0x00, //     Logical Minium(0)
//     0x26, 0xF5, 0x00, //     Logical Maximum(0xF5)
//     0x05, 0x0C, //     Usage Page (Consumer),
//     0x19, 0x00, //     Usage Minium(0)
//     0x2A, 0xF5, 0x00, //     Usage Maximum(0xF5)
//     0x81, 0x00, //     Input (Array, Data, Variable)
//     0xC0, // End Collection
// ];

///Fixed functionality consumer control report descriptor
/// 
/// Based on [Logitech Gaming Keyboard](http://www.usblyzer.com/reports/usb-properties/usb-keyboard.html)
/// dumped by [USBlyzer](http://www.usblyzer.com/)
/// 
/// Single bit packed `u8` report
/// * Bit 0 - Scan Next Track
/// * Bit 1 - Scan Previous Track
/// * Bit 2 - Stop
/// * Bit 3 - Play/Pause
/// * Bit 4 - Mute
/// * Bit 5 - Volume Increment
/// * Bit 6 - Volume Decrement
/// * Bit 7 - Reserved
#[rustfmt::skip]
pub const FIXED_FUNCTION_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x0C, //        Usage Page (Consumer Devices)  
    0x09, 0x01, //        Usage (Consumer Control)  
    0xA1, 0x01, //        Collection (Application)  
    0x05, 0x0C, //            Usage Page (Consumer Devices)  
    0x15, 0x00, //            Logical Minimum (0)  
    0x25, 0x01, //            Logical Maximum (1)  
    0x75, 0x01, //            Report Size (1)  
    0x95, 0x07, //            Report Count (7)  
    0x09, 0xB5, //            Usage (Scan Next Track)  
    0x09, 0xB6, //            Usage (Scan Previous Track)  
    0x09, 0xB7, //            Usage (Stop)  
    0x09, 0xCD, //            Usage (Play/Pause)  
    0x09, 0xE2, //            Usage (Mute)  
    0x09, 0xE9, //            Usage (Volume Increment)  
    0x09, 0xEA, //            Usage (Volume Decrement)  
    0x81, 0x02, //            Input (Data,Var,Abs,NWrp,Lin,Pref,NNul,Bit)  
    0x95, 0x01, //            Report Count (1)  
    0x81, 0x01, //            Input (Cnst,Ary,Abs)  
    0xC0, //        End Collection
];

/// Create a pre-configured [`crate::hid_class::UsbHidClassBuilder`] for a consumer control
pub fn new_consumer_control<B: usb_device::bus::UsbBus>(
    usb_alloc: &'_ UsbBusAllocator<B>,
) -> UsbHidClassBuilder<'_, B> {
    UsbHidClassBuilder::new(usb_alloc, MULTIPLE_CODE_REPORT_DESCRIPTOR)
        .interface_description("Consumer Control")
        .idle_default(Milliseconds(0))
        .unwrap()
        .in_endpoint(UsbPacketSize::Size8, Milliseconds(20))
        .unwrap()
        .without_out_endpoint()
}

/// Provides an interface to send consumer control codes to
/// the host device
pub trait HidConsumerControl {
    /// Writes an input report given representing the update to the mouse state
    /// to the host system
    fn write_consumer_control_report(
        &self,
        codes: &[crate::usage::Consumer],
    ) -> usb_device::Result<()>;
}

impl<B: UsbBus> HidConsumerControl for UsbHidClass<'_, B> {
    fn write_consumer_control_report(
        &self,
        codes: &[crate::usage::Consumer],
    ) -> core::result::Result<(), usb_device::UsbError> {
        let mut report = [0; 8];
        let mut i = 0;
        for &c in codes
            .iter()
            .filter(|&&c| c != crate::usage::Consumer::Unassigned)
        {
            if i > report.len() {
                break;
            }
            report[i..i + 2].copy_from_slice(&(c as u16).to_le_bytes());
            i += 2;
        }

        match self.write_report(&report) {
            Ok(8) => Ok(()),
            Ok(n) => {
                warn!("Sent {:X} bytes, expected 8 byte", n,);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}
