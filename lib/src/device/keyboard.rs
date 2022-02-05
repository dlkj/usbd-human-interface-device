//!HID keyboards

use crate::hid_class::interface::InterfaceConfig;
use embedded_time::duration::Milliseconds;
use frunk::{HCons, HNil};
use packed_struct::prelude::*;
use usb_device::class_prelude::*;

use crate::hid_class::prelude::*;
use crate::page::Keyboard;

/// Create a pre-configured [`crate::hid_class::UsbHidClassBuilder`] for a boot keyboard
pub fn new_boot_keyboard<B: usb_device::bus::UsbBus>(
    usb_alloc: &'_ UsbBusAllocator<B>,
) -> UsbHidClassBuilder<'_, B, HCons<InterfaceConfig<'_>, HNil>> {
    UsbHidClassBuilder::new(usb_alloc)
        .new_interface(
            UsbHidInterfaceBuilder::new(BOOT_KEYBOARD_REPORT_DESCRIPTOR)
                .boot_device(InterfaceProtocol::Keyboard)
                .description("Keyboard")
                .idle_default(Milliseconds(500))
                .unwrap()
                .in_endpoint(UsbPacketSize::Size8, Milliseconds(10))
                .unwrap()
                //.without_out_endpoint()
                //Shouldn't require a dedicated out endpoint, but leds are flaky without it
                .with_out_endpoint(UsbPacketSize::Size8, Milliseconds(100))
                .unwrap()
                .build(),
        )
        .unwrap()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, PackedStruct)]
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

//25 bytes
//byte 0 - modifiers
//byte 1 - reserved 0s
//byte 2-7 - array of keycodes - used for boot support
//byte 9-24 - bit array of pressed keys
#[rustfmt::skip]
pub const NKRO_BOOT_KEYBOARD_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x01,                     // Usage Page (Generic Desktop),
    0x09, 0x06,                     // Usage (Keyboard),
    0xA1, 0x01,                     // Collection (Application),
    // bitmap of modifiers
    0x75, 0x01,                     //   Report Size (1),
    0x95, 0x08,                     //   Report Count (8),
    0x05, 0x07,                     //   Usage Page (Key Codes),
    0x19, 0xE0,                     //   Usage Minimum (224),
    0x29, 0xE7,                     //   Usage Maximum (231),
    0x15, 0x00,                     //   Logical Minimum (0),
    0x25, 0x01,                     //   Logical Maximum (1),
    0x81, 0x02,                     //   Input (Data, Variable, Absolute), ;Modifier byte
    // 7 bytes of padding
    0x75, 0x38,                     //   Report Size (0x38),
    0x95, 0x01,                     //   Report Count (1),
    0x81, 0x01,                     //   Input (Constant), ;Reserved byte
    // LED output report
    0x95, 0x05,                     //   Report Count (5),
    0x75, 0x01,                     //   Report Size (1),
    0x05, 0x08,                     //   Usage Page (LEDs),
    0x19, 0x01,                     //   Usage Minimum (1),
    0x29, 0x05,                     //   Usage Maximum (5),
    0x91, 0x02,                     //   Output (Data, Variable, Absolute),
    0x95, 0x01,                     //   Report Count (1),
    0x75, 0x03,                     //   Report Size (3),
    0x91, 0x03,                     //   Output (Constant),
    // bitmap of keys
    0x95, 0x88,                     //   Report Count () - (REPORT_BYTES-1)*8
    0x75, 0x01,                     //   Report Size (1),
    0x15, 0x00,                     //   Logical Minimum (0),
    0x25, 0x01,                     //   Logical Maximum(1),
    0x05, 0x07,                     //   Usage Page (Key Codes),
    0x19, 0x00,                     //   Usage Minimum (0),
    0x29, 0x87,                     //   Usage Maximum (), - (REPORT_BYTES-1)*8-1
    0x81, 0x02,                     //   Input (Data, Variable, Absolute),
    0xc0                            // End Collection
];

#[derive(Clone, Copy, Debug, PartialEq, PackedStruct)]
#[packed_struct(endian = "lsb", bit_numbering = "msb0", size_bytes = "25")]
pub struct NKROBootKeyboardReport {
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
    pub boot_keys: [Keyboard; 6],
    //The usb lsb/lsb0 expected ordering isn't compatible with pact structs
    #[packed_field(bytes = "8..25", element_size_bits = "8")]
    pub nkro_keys: [u8; 17],
}

impl NKROBootKeyboardReport {
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
            boot_keys: [Keyboard::NoEventIndicated; 6],
            nkro_keys: [0; 17],
        };

        let mut boot_keys_error = false;
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
                    report.nkro_keys[0] |= 1 << k as u8;

                    if !boot_keys_error {
                        boot_keys_error = true;
                        i = report.boot_keys.len();
                        report.boot_keys.fill(k);
                    }
                }
                _ => {
                    if (k as usize) < report.nkro_keys.len() * 8 {
                        let byte = (k as usize) / 8;
                        let bit = (k as u8) % 8;
                        report.nkro_keys[byte] |= 1 << bit;
                    }

                    if boot_keys_error {
                        continue;
                    }

                    if i < report.boot_keys.len() {
                        report.boot_keys[i] = k;
                        i += 1;
                    } else {
                        boot_keys_error = true;
                        i = report.boot_keys.len();
                        report.boot_keys.fill(Keyboard::ErrorRollOver);
                    }
                }
            }
        }
        report
    }
}

//18 bytes - derived from https://learn.adafruit.com/custom-hid-devices-in-circuitpython/n-key-rollover-nkro-hid-device
//First byte modifiers, 17 byte key bit array
#[rustfmt::skip]
pub const NKRO_COMPACT_KEYBOARD_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x01,                     // Usage Page (Generic Desktop),
    0x09, 0x06,                     // Usage (Keyboard),
    0xA1, 0x01,                     // Collection (Application),
    // bitmap of modifiers
    0x75, 0x01,                     //   Report Size (1),
    0x95, 0x08,                     //   Report Count (8),
    0x05, 0x07,                     //   Usage Page (Key Codes),
    0x19, 0xE0,                     //   Usage Minimum (224),
    0x29, 0xE7,                     //   Usage Maximum (231),
    0x15, 0x00,                     //   Logical Minimum (0),
    0x25, 0x01,                     //   Logical Maximum (1),
    0x81, 0x02,                     //   Input (Data, Variable, Absolute), ;Modifier byte
    // LED output report
    0x95, 0x05,                     //   Report Count (5),
    0x75, 0x01,                     //   Report Size (1),
    0x05, 0x08,                     //   Usage Page (LEDs),
    0x19, 0x01,                     //   Usage Minimum (1),
    0x29, 0x05,                     //   Usage Maximum (5),
    0x91, 0x02,                     //   Output (Data, Variable, Absolute),
    0x95, 0x01,                     //   Report Count (1),
    0x75, 0x03,                     //   Report Size (3),
    0x91, 0x03,                     //   Output (Constant),
    // bitmap of keys
    0x95, 0x88,                     //   Report Count () - (REPORT_BYTES-1)*8
    0x75, 0x01,                     //   Report Size (1),
    0x15, 0x00,                     //   Logical Minimum (0),
    0x25, 0x01,                     //   Logical Maximum(1),
    0x05, 0x07,                     //   Usage Page (Key Codes),
    0x19, 0x00,                     //   Usage Minimum (0),
    0x29, 0x87,                     //   Usage Maximum (), - (REPORT_BYTES-1)*8-1
    0x81, 0x02,                     //   Input (Data, Variable, Absolute),
    0xc0                            // End Collection
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
