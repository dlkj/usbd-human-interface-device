/// Implements a Hid Keyboard that conforms to the Boot specification. This aims
/// to be compatible with BIOS and other reduced functionality USB hosts
///
/// This is defined in Appendix B.1 of Device Class Definition for Human
/// Interface Devices (Hid) Version 1.11 -
/// https://www.usb.org/sites/default/files/hid1_11.pdf - Appendix B.1, E.6
#[rustfmt::skip] 
pub const HID_BOOT_KEYBOARD_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x01,         // Usage Page (Generic Desktop),
    0x09, 0x06,         // Usage (Keyboard),
    0xA1, 0x01,         // Collection (Application),
    0x75, 0x01,         //     Report Size (1),
    0x95, 0x08,         //     Report Count (8),
    0x05, 0x07,         //     Usage Page (Key Codes),
    0x19, 0xE0,         //     Usage Minimum (224),
    0x29, 0xE7,         //     Usage Maximum (231),
    0x15, 0x00,         //     Logical Minimum (0),
    0x25, 0x01,         //     Logical Maximum (1),
    0x81, 0x02,         //     Input (Data, Variable, Absolute), ;Modifier byte
    0x95, 0x01,         //     Report Count (1),
    0x75, 0x08,         //     Report Size (8),
    0x81, 0x01,         //     Input (Constant), ;Reserved byte
    0x95, 0x05,         //     Report Count (5),
    0x75, 0x01,         //     Report Size (1),
    0x05, 0x08,         //     Usage Page (LEDs),
    0x19, 0x01,         //     Usage Minimum (1),
    0x29, 0x05,         //     Usage Maximum (5),
    0x91, 0x02,         //     Output (Data, Variable, Absolute), ;LED report
    0x95, 0x01,         //     Report Count (1),
    0x75, 0x03,         //     Report Size (3),
    0x91, 0x01,         //     Output (Constant), ;LED report padding
    0x95, 0x06,         //     Report Count (6),
    0x75, 0x08,         //     Report Size (8),
    0x15, 0x00,         //     Logical Minimum (0),
    0x26, 0xFF, 0x00,   //     Logical Maximum(255),
    0x05, 0x07,         //     Usage Page (Key Codes),
    0x19, 0x00,         //     Usage Minimum (0),
    0x2A, 0xFF, 0x00,   //     Usage Maximum (255),
    0x81, 0x00,         //     Input (Data, Array),
    0xC0,               // End Collection
];
