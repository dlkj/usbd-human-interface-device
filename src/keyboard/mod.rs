//!Implements HID keyboard devices
pub mod descriptors;

use log::{error, warn};
use usb_device::class_prelude::*;
use usb_device::Result;

/// HIDKeyboard provides an interface to send keycodes to the host device and
/// receive LED status information
pub trait HIDKeyboard {
    /// Writes an input report given representing keycodes to the host system
    fn write_keycodes<K>(&self, keycodes: K) -> Result<()>
    where
        K: IntoIterator<Item = u8>;

    /// Read LED status from the host system
    fn read_leds(&self) -> Result<u8>;
}

/// Implements a HID Keyboard that conforms to the Boot specification. This aims
/// to be compatible with BIOS and other reduced functionality USB hosts
///
/// This is defined in Appendix B.1 of Device Class Definition for Human
/// Interface Devices (HID) Version 1.11 -
/// https://www.usb.org/sites/default/files/hid1_11.pdf
#[derive(Default)]
pub struct HIDBootKeyboard {}

impl super::hid::HIDClass for HIDBootKeyboard {
    fn packet_size(&self) -> u8 {
        8
    }
    fn poll_interval(&self) -> embedded_time::duration::Milliseconds {
        embedded_time::duration::Milliseconds(20)
    }
    fn report_descriptor(&self) -> &'static [u8] {
        &descriptors::HID_BOOT_KEYBOARD_REPORT_DESCRIPTOR
    }
    fn interface_protocol(&self) -> u8 {
        super::hid::InterfaceProtocol::Keyboard as u8
    }
    fn reset(&mut self) {}
}

impl super::hid::HIDProtocolSupport for HIDBootKeyboard {
    fn set_protocol(&mut self, _: u8) {
        todo!()
    }
    fn get_protocol(&self) -> u8 {
        todo!()
    }
}

impl<B: UsbBus> HIDKeyboard for super::hid::HID<'_, B, HIDBootKeyboard> {
    fn write_keycodes<K>(&self, keycodes: K) -> core::result::Result<(), usb_device::UsbError>
    where
        K: IntoIterator<Item = u8>,
    {
        let mut data = [0; 8];

        let mut reporting_error = false;
        let mut key_report_idx = 2;

        for k in keycodes {
            if k == 0x00 {
                //ignore none keycode
            } else if (0xE0..=0xE7).contains(&k) {
                //modifiers
                data[0] |= 1 << (k - 0xE0);
            } else if !reporting_error {
                //only do other keycodes if we aren't already sending an error
                if k < 0x04 {
                    //Fill report if any keycode is an error
                    (&mut data[2..]).fill(k);
                    reporting_error = true;
                } else if key_report_idx < data.len() {
                    //Report a standard keycode
                    data[key_report_idx] = k;
                    key_report_idx += 1;
                } else {
                    //Rollover if full
                    (&mut data[2..]).fill(0x1);
                    reporting_error = true;
                }
            }
        }

        match self.write_report(&data) {
            Ok(8) => Ok(()),
            Ok(n) => {
                warn!(
                    "interface {:X} sent {:X} bytes, expected 8 byte",
                    n,
                    u8::from(self.interface_number()),
                );
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    fn read_leds(&self) -> core::result::Result<u8, usb_device::UsbError> {
        let mut data = [0; 8];
        match self.read_report(&mut data) {
            Ok(0) => {
                error!(
                    "interface {:X} received zero length report, expected 1 byte",
                    u8::from(self.interface_number()),
                );
                Err(UsbError::ParseError)
            }
            Ok(_) => Ok(data[0]),
            Err(e) => Err(e),
        }
        //todo convert to bitflags
    }
}
