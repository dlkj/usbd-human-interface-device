//!HID keyboards

use embedded_time::duration::Milliseconds;
use log::{error, warn};
use packed_struct::prelude::*;
use usb_device::class_prelude::*;
use usb_device::Result;

use crate::hid_class::prelude::*;
use crate::page::Keyboard;

/// Create a pre-configured [`crate::hid_class::UsbHidClassBuilder`] for a boot keyboard
pub fn new_boot_keyboard<B: usb_device::bus::UsbBus>(
    usb_alloc: &'_ UsbBusAllocator<B>,
) -> UsbHidClassBuilder<'_, B> {
    UsbHidClassBuilder::new(usb_alloc, BOOT_KEYBOARD_REPORT_DESCRIPTOR)
        .boot_device(InterfaceProtocol::Keyboard)
        .interface_description("Keyboard")
        .idle_default(Milliseconds(500))
        .unwrap()
        .in_endpoint(UsbPacketSize::Size8, Milliseconds(20))
        .unwrap()
        .without_out_endpoint()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PackedStruct)]
#[packed_struct(endian = "lsb", bit_numbering = "lsb0", size_bytes = "1")]
pub struct KeyboardLeds {
    #[packed_field(bits = "0")]
    pub num_lock: bool,
    #[packed_field(bits = "1")]
    pub caps_lock: bool,
    #[packed_field(bits = "2")]
    pub scroll_lock: bool,
    #[packed_field(bits = "3")]
    pub compose: bool,
    #[packed_field(bits = "4")]
    pub kana: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, PackedStruct)]
#[packed_struct(endian = "lsb", bit_numbering = "msb0", size_bytes = "8")]
pub struct BootKeyboardReport {
    #[packed_field(bits = "0")]
    pub right_gui: bool,
    #[packed_field(bits = "1")]
    pub right_alt: bool,
    #[packed_field(bits = "2")]
    pub right_shift: bool,
    #[packed_field(bits = "3")]
    pub right_ctrl: bool,
    #[packed_field(bits = "4")]
    pub left_gui: bool,
    #[packed_field(bits = "5")]
    pub left_alt: bool,
    #[packed_field(bits = "6")]
    pub left_shift: bool,
    #[packed_field(bits = "7")]
    pub left_ctrl: bool,
    #[packed_field(bytes = "2..8", ty = "enum", element_size_bytes = "1")]
    pub keys: [Keyboard; 6],
}

impl BootKeyboardReport {
    pub fn new<K: IntoIterator<Item = Keyboard>>(keys: K) -> Self {
        let mut report = Self {
            left_ctrl: false,
            left_shift: false,
            left_alt: false,
            left_gui: false,
            right_ctrl: false,
            right_shift: false,
            right_alt: false,
            right_gui: false,
            keys: [Keyboard::NoEventIndicated; 6],
        };

        let mut error = false;
        let mut i = 0;
        for k in keys.into_iter() {
            match k {
                Keyboard::LeftControl => {
                    report.left_ctrl = true;
                }
                Keyboard::LeftShift => {
                    report.left_shift = true;
                }
                Keyboard::LeftAlt => {
                    report.left_alt = true;
                }
                Keyboard::LeftGUI => {
                    report.left_gui = true;
                }
                Keyboard::RightControl => {
                    report.right_ctrl = true;
                }
                Keyboard::RightShift => {
                    report.right_shift = true;
                }
                Keyboard::RightAlt => {
                    report.right_alt = true;
                }
                Keyboard::RightGUI => {
                    report.right_gui = true;
                }
                Keyboard::NoEventIndicated => {}
                Keyboard::ErrorRollOver | Keyboard::POSTFail | Keyboard::ErrorUndefine => {
                    if !error {
                        error = true;
                        i = report.keys.len();
                        report.keys.fill(k);
                    }
                }
                _ => {
                    if error {
                        continue;
                    }

                    if i < report.keys.len() {
                        report.keys[i] = k;
                        i += 1;
                    } else {
                        error = true;
                        i = report.keys.len();
                        report.keys.fill(Keyboard::ErrorRollOver);
                    }
                }
            }
        }
        report
    }
}

/// Provides an interface to send keycodes to the host device and
/// receive LED status information
pub trait HidKeyboard {
    /// Writes an input report to the host system indicating the stat of the keyboard
    fn write_keyboard_report<K>(&self, keycodes: K) -> Result<()>
    where
        K: IntoIterator<Item = Keyboard>;

    /// Read LED status from the host system
    fn read_keyboard_report(&self) -> Result<KeyboardLeds>;
}

impl<B: UsbBus> HidKeyboard for UsbHidClass<'_, B> {
    fn write_keyboard_report<K>(
        &self,
        keycodes: K,
    ) -> core::result::Result<(), usb_device::UsbError>
    where
        K: IntoIterator<Item = Keyboard>,
    {
        let mut data = [0; 8];

        let mut reporting_error = false;
        let mut key_report_idx = 2;

        for k in keycodes {
            if k == Keyboard::NoEventIndicated {
                //ignore none keycode
            } else if (Keyboard::LeftControl..=Keyboard::RightGUI).contains(&k) {
                //modifiers
                data[0] |= 1 << (k as u8 - Keyboard::LeftControl as u8);
            } else if !reporting_error {
                //only do other keycodes if we aren't already sending an error
                if k < Keyboard::A {
                    //Fill report if any keycode is an error
                    (&mut data[2..]).fill(k as u8);
                    reporting_error = true;
                } else if key_report_idx < data.len() {
                    //Report a standard keycode
                    data[key_report_idx] = k as u8;
                    key_report_idx += 1;
                } else {
                    //Rollover if full
                    (&mut data[2..]).fill(Keyboard::ErrorRollOver as u8);
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

    fn read_keyboard_report(&self) -> core::result::Result<KeyboardLeds, usb_device::UsbError> {
        let mut data = [0; 1];
        match self.read_report(&mut data) {
            Ok(0) => {
                error!("received zero length report, expected 1 byte",);
                Err(UsbError::ParseError)
            }
            Ok(_) => KeyboardLeds::unpack(&data).map_err(|_| UsbError::ParseError),
            Err(e) => Err(e),
        }
    }
}

/// HID Keyboard report descriptor conforming to the Boot specification
/// 
/// This aims to be compatible with BIOS and other reduced functionality USB hosts
///
/// This is defined in Appendix B.1 & E.6 of [Device Class Definition for Human
/// Interface Devices (Hid) Version 1.11](<https://www.usb.org/sites/default/files/hid1_11.pdf>)
#[rustfmt::skip]
pub const BOOT_KEYBOARD_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x01, // Usage Page (Generic Desktop),
    0x09, 0x06, // Usage (Keyboard),
    0xA1, 0x01, // Collection (Application),
    0x75, 0x01, //     Report Size (1),
    0x95, 0x08, //     Report Count (8),
    0x05, 0x07, //     Usage Page (Key Codes),
    0x19, 0xE0, //     Usage Minimum (224),
    0x29, 0xE7, //     Usage Maximum (231),
    0x15, 0x00, //     Logical Minimum (0),
    0x25, 0x01, //     Logical Maximum (1),
    0x81, 0x02, //     Input (Data, Variable, Absolute), ;Modifier byte
    0x95, 0x01, //     Report Count (1),
    0x75, 0x08, //     Report Size (8),
    0x81, 0x01, //     Input (Constant), ;Reserved byte
    0x95, 0x05, //     Report Count (5),
    0x75, 0x01, //     Report Size (1),
    0x05, 0x08, //     Usage Page (LEDs),
    0x19, 0x01, //     Usage Minimum (1),
    0x29, 0x05, //     Usage Maximum (5),
    0x91, 0x02, //     Output (Data, Variable, Absolute), ;LED report
    0x95, 0x01, //     Report Count (1),
    0x75, 0x03, //     Report Size (3),
    0x91, 0x01, //     Output (Constant), ;LED report padding
    0x95, 0x06, //     Report Count (6),
    0x75, 0x08, //     Report Size (8),
    0x15, 0x00, //     Logical Minimum (0),
    0x26, 0xFF, 0x00, //     Logical Maximum(255),
    0x05, 0x07, //     Usage Page (Key Codes),
    0x19, 0x00, //     Usage Minimum (0),
    0x2A, 0xFF, 0x00, //     Usage Maximum (255),
    0x81, 0x00, //     Input (Data, Array),
    0xC0, // End Collection
];

#[cfg(test)]
mod test {
    use packed_struct::prelude::*;

    use crate::device::keyboard::{BootKeyboardReport, KeyboardLeds};
    use crate::page::Keyboard;

    #[test]
    fn leds_num_lock() {
        assert_eq!(
            KeyboardLeds::unpack(&[1]),
            Ok(KeyboardLeds {
                num_lock: true,
                caps_lock: false,
                scroll_lock: false,
                compose: false,
                kana: false,
            })
        );
    }

    #[test]
    fn leds_caps_lock() {
        assert_eq!(
            KeyboardLeds::unpack(&[2]),
            Ok(KeyboardLeds {
                num_lock: false,
                caps_lock: true,
                scroll_lock: false,
                compose: false,
                kana: false,
            })
        );

        assert_eq!(
            KeyboardLeds {
                num_lock: false,
                caps_lock: true,
                scroll_lock: false,
                compose: false,
                kana: false,
            }
            .pack(),
            Ok([2])
        );
    }

    #[test]
    fn boot_keyboard_report_mixed() {
        let bytes = BootKeyboardReport::new([
            Keyboard::LeftAlt,
            Keyboard::A,
            Keyboard::B,
            Keyboard::C,
            Keyboard::RightGUI,
        ])
        .pack()
        .unwrap();

        assert_eq!(
            bytes,
            [
                0x1_u8 << (Keyboard::LeftAlt as u8 - Keyboard::LeftControl as u8)
                    | 0x1_u8 << (Keyboard::RightGUI as u8 - Keyboard::LeftControl as u8),
                0,
                Keyboard::A as u8,
                Keyboard::B as u8,
                Keyboard::C as u8,
                0,
                0,
                0
            ]
        );
    }

    #[test]
    fn boot_keyboard_report_keys() {
        let bytes = BootKeyboardReport::new([
            Keyboard::A,
            Keyboard::B,
            Keyboard::C,
            Keyboard::D,
            Keyboard::E,
            Keyboard::F,
        ])
        .pack()
        .unwrap();

        assert_eq!(
            bytes,
            [
                0,
                0,
                Keyboard::A as u8,
                Keyboard::B as u8,
                Keyboard::C as u8,
                Keyboard::D as u8,
                Keyboard::E as u8,
                Keyboard::F as u8
            ]
        );
    }

    #[test]
    fn boot_keyboard_report_rollover() {
        let bytes = BootKeyboardReport::new([
            Keyboard::LeftAlt,
            Keyboard::A,
            Keyboard::B,
            Keyboard::C,
            Keyboard::D,
            Keyboard::E,
            Keyboard::F,
            Keyboard::G,
            Keyboard::RightGUI,
        ])
        .pack()
        .unwrap();

        assert_eq!(
            bytes,
            [
                0x1_u8 << (Keyboard::LeftAlt as u8 - Keyboard::LeftControl as u8)
                    | 0x1_u8 << (Keyboard::RightGUI as u8 - Keyboard::LeftControl as u8),
                0,
                Keyboard::ErrorRollOver as u8,
                Keyboard::ErrorRollOver as u8,
                Keyboard::ErrorRollOver as u8,
                Keyboard::ErrorRollOver as u8,
                Keyboard::ErrorRollOver as u8,
                Keyboard::ErrorRollOver as u8,
            ]
        );
    }
}
