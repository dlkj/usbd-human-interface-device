//!Implements Hid keyboard devices
pub mod descriptors;

use crate::hid::HidConfig;
use crate::hid::InterfaceProtocol;
use crate::hid::InterfaceSubClass;
use crate::hid::UsbHidClass;
use log::{error, warn};
use usb_device::class_prelude::*;
use usb_device::Result;

/// HidKeyboard provides an interface to send keycodes to the host device and
/// receive LED status information
pub trait HidKeyboard {
    /// Writes an input report given representing keycodes to the host system
    fn write_keycodes<K>(&self, keycodes: K) -> Result<()>
    where
        K: IntoIterator<Item = u8>;

    /// Read LED status from the host system
    fn read_leds(&self) -> Result<u8>;
}

/// Implements a Hid Keyboard that conforms to the Boot specification. This aims
/// to be compatible with BIOS and other reduced functionality USB hosts
///
/// This is defined in Appendix B.1 of Device Class Definition for Human
/// Interface Devices (Hid) Version 1.11 -
/// https://www.usb.org/sites/default/files/hid1_11.pdf
#[derive(Default)]
pub struct HidBootKeyboard {}

impl HidConfig for HidBootKeyboard {
    fn packet_size(&self) -> u8 {
        8
    }
    fn poll_interval(&self) -> embedded_time::duration::Milliseconds {
        embedded_time::duration::Milliseconds(20)
    }
    fn report_descriptor(&self) -> &'static [u8] {
        &descriptors::HID_BOOT_KEYBOARD_REPORT_DESCRIPTOR
    }
    fn interface_protocol(&self) -> InterfaceProtocol {
        InterfaceProtocol::Keyboard
    }
    fn interface_sub_class(&self) -> InterfaceSubClass {
        InterfaceSubClass::Boot
    }
    fn reset(&mut self) {}
    fn interface_name(&self) -> &str {
        "Keyboard"
    }
}

impl<B: UsbBus> HidKeyboard for UsbHidClass<'_, B, HidBootKeyboard> {
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
                warn!("sent {:X} bytes, expected 8 byte", n,);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    fn read_leds(&self) -> core::result::Result<u8, usb_device::UsbError> {
        let mut data = [0; 8];
        match self.read_report(&mut data) {
            Ok(0) => {
                error!("received zero length report, expected 1 byte",);
                Err(UsbError::ParseError)
            }
            Ok(_) => Ok(data[0]),
            Err(e) => Err(e),
        }
        //todo convert to bitflags
    }
}
