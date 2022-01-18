//!HID consumer control devices

#[rustfmt::skip]
pub const HID_CONSUMER_CONTROL_SINGLE_ARRAY_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x0C, // Usage Page (Consumer),
    0x09, 0x01, // Usage (Consumer Control),
    0xA1, 0x01, // Collection (Application),
    0x75, 0x08, //     Report Size(8)
    0x95, 0x07, //     Report Count(1)
    0x15, 0x00, //     Logical Minium(0)
    0x26, 0xF5, 0x00, //     Logical Maximum(0xF5)
    0x05, 0x0C, //     Usage Page (Consumer),
    0x19, 0x00, //     Usage Minium(0)
    0x2A, 0xF5, 0x00, //     Usage Maximum(0xF5)
    0x81, 0x00, //     Input (Array, Data, Variable)
    0xC0, // End Collection
];

#[rustfmt::skip]
pub const HID_CONSUMER_CONTROL_8_ARRAY_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x0C, // Usage Page (Consumer),
    0x09, 0x01, // Usage (Consumer Control),
    0xA1, 0x01, // Collection (Application),
    0x75, 0x08, //     Report Size(8)
    0x95, 0x07, //     Report Count(8)
    0x15, 0x00, //     Logical Minium(0)
    0x26, 0xF5, 0x00, //     Logical Maximum(0xF5)
    0x05, 0x0C, //     Usage Page (Consumer),
    0x19, 0x00, //     Usage Minium(0)
    0x2A, 0xF5, 0x00, //     Usage Maximum(0xF5)
    0x81, 0x00, //     Input (Array, Data, Variable)
    0xC0, // End Collection
];

#[rustfmt::skip]
pub const HID_CONSUMER_CONTROL_VOL_KNOB_7_ARRAY_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x0C, // Usage Page (Consumer),
    0x09, 0x01, // Usage (Consumer Control),
    0xA1, 0x01, // Collection (Application),
                //
    0x09, 0xE0, //     Usage (Volume)
    0x15, 0x9C, //     Logical Minimum(-100)
    0x25, 0x64, //     Logical Maximum(100)
    0x95, 0x01, //     ReportCount(1),
    0x75, 0x08, //     ReportSize(8),
    0x81, 0x06, //     Input (Data, Variable, Relative)
                //
    0x75, 0x08, //     Report Size(8)
    0x95, 0x07, //     Report Count(7)
    0x15, 0x00, //     Logical Minium(0)
    0x26, 0xF5, 0x00, //     Logical Maximum(0xF5)
    0x05, 0x0C, //     Usage Page (Consumer),
    0x19, 0x00, //     Usage Minium(0)
    0x2A, 0xF5, 0x00, //     Usage Maximum(0xF5)
    0x81, 0x00, //     Input (Array, Data, Variable)
    0xC0, // End Collection
];
