//!HID keyboards

use crate::hid_class::interface::{InterfaceClass, RawInterface, WrappedInterfaceConfig};
use core::cell::RefCell;
use delegate::delegate;
use embedded_time::duration::Milliseconds;
use embedded_time::timer::param::{OneShot, Running};
use embedded_time::{Clock, Timer};
use log::error;
use packed_struct::prelude::*;
use usb_device::class_prelude::*;
use usb_device::UsbError;

use crate::hid_class::prelude::*;
use crate::page::Keyboard;

pub struct BootKeyboardInterface<'a, B: UsbBus> {
    inner: RawInterface<'a, B>,
}

impl<'a, B: UsbBus> BootKeyboardInterface<'a, B> {
    pub fn write_report(&self, report: &BootKeyboardReport) -> usb_device::Result<usize> {
        let data = report.pack().map_err(|e| {
            error!("Error packing BootKeyboardReport: {:?}", e);
            UsbError::ParseError
        })?;
        self.inner.write_report(&data)
    }

    pub fn read_report(&self) -> usb_device::Result<KeyboardLedsReport> {
        let data = &mut [0];
        match self.inner.read_report(data) {
            Err(e) => Err(e),
            Ok(_) => match KeyboardLedsReport::unpack(data) {
                Ok(r) => Ok(r),
                Err(_) => Err(UsbError::ParseError),
            },
        }
    }

    delegate! {
        to self.inner{
            pub fn global_idle(&self) -> Milliseconds;
        }
    }

    pub fn default_config() -> WrappedInterfaceConfig<'a, Self> {
        WrappedInterfaceConfig::new(
            InterfaceBuilder::new(BOOT_KEYBOARD_REPORT_DESCRIPTOR)
                .boot_device(InterfaceProtocol::Keyboard)
                .description("Keyboard")
                .idle_default(Milliseconds(500))
                .unwrap()
                .in_endpoint(UsbPacketSize::Bytes8, Milliseconds(10))
                .unwrap()
                //.without_out_endpoint()
                //Shouldn't require a dedicated out endpoint, but leds are flaky without it
                .with_out_endpoint(UsbPacketSize::Bytes8, Milliseconds(100))
                .unwrap()
                .build(),
            (),
        )
    }
}

impl<'a, B: UsbBus> InterfaceClass<'a> for BootKeyboardInterface<'a, B> {
    delegate! {
        to self.inner{
           fn report_descriptor(&self) -> &'a [u8];
           fn id(&self) -> InterfaceNumber;
           fn write_descriptors(&self, writer: &mut DescriptorWriter) -> usb_device::Result<()>;
           fn get_string(&self, index: StringIndex, _lang_id: u16) -> Option<&'static str>;
           fn reset(&mut self);
           fn set_report(&mut self, data: &[u8]) -> usb_device::Result<()>;
           fn get_report(&mut self, data: &mut [u8]) -> usb_device::Result<usize>;
           fn get_report_ack(&mut self) -> usb_device::Result<()>;
           fn set_idle(&mut self, report_id: u8, value: u8);
           fn get_idle(&self, report_id: u8) -> u8;
           fn set_protocol(&mut self, protocol: HidProtocol);
           fn get_protocol(&self) -> HidProtocol;
        }
    }
}

impl<'a, B: UsbBus> WrappedInterface<'a, B> for BootKeyboardInterface<'a, B> {
    fn new(interface: RawInterface<'a, B>, _: ()) -> Self {
        Self { inner: interface }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, PackedStruct)]
#[packed_struct(endian = "lsb", bit_numbering = "lsb0", size_bytes = "1")]
pub struct KeyboardLedsReport {
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

#[derive(Clone, Copy, Debug, PartialEq, Default, PackedStruct)]
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
        let mut report = Self::default();

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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default, PackedStruct)]
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
        let mut report = Self::default();

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

#[derive(Debug)]
pub enum UsbHidError {
    WouldBlock,
    Duplicate,
    UsbError(UsbError),
    SerializationError,
}

struct IdleManager<'a, R, C: Clock> {
    clock: &'a C,
    last_report: Option<R>,
    current: Milliseconds,
    default: Milliseconds,
    timer: Option<Timer<'a, OneShot, Running, C, Milliseconds>>,
}

impl<'a, R: Eq + Copy, C: Clock<T = u64>> IdleManager<'a, R, C> {
    pub fn new(clock: &'a C, default: Milliseconds) -> Self {
        Self {
            clock,
            last_report: None,
            current: default,
            default,
            timer: None,
        }
    }

    pub fn reset(&mut self) {
        self.last_report = None;
        self.timer = None;
        self.current = self.default;
    }

    pub fn report_written(&mut self, report: R) {
        self.last_report = Some(report);
        self.reset_timer();
    }

    pub fn reset_timer(&mut self) {
        if self.current == Milliseconds(0u32) {
            self.timer = None
        } else {
            self.timer = Some(self.clock.new_timer(self.current).start().unwrap())
        }
    }

    pub fn set_duration(&mut self, duration: Milliseconds) {
        self.current = duration;

        if let Some(t) = self.timer.as_ref() {
            let elapsed: Milliseconds = t.elapsed().unwrap();
            if elapsed > self.current {
                //sending a report is now overdue, set a zero time timer
                self.timer = Some(self.clock.new_timer(Milliseconds(0)).start().unwrap())
            } else {
                //carry over elapsed time
                self.timer = Some(
                    self.clock
                        .new_timer(self.current - elapsed)
                        .start()
                        .unwrap(),
                )
            }
        } else {
            self.reset_timer();
        }
    }

    pub fn is_duplicate(&self, report: &R) -> bool {
        self.last_report.as_ref() == Some(report)
    }

    pub fn is_idle_expired(&self) -> bool {
        self.timer
            .as_ref()
            .map(|t| t.is_expired().unwrap())
            .unwrap_or_default()
    }
}

pub struct NKROBootKeyboardInterface<'a, B: UsbBus, C: Clock<T = u64>> {
    inner: RawInterface<'a, B>,
    idle_manager: RefCell<IdleManager<'a, NKROBootKeyboardReport, C>>,
}

impl<'a, B: UsbBus, C: Clock<T = u64>> NKROBootKeyboardInterface<'a, B, C> {
    pub fn write_report(&self, report: &NKROBootKeyboardReport) -> Result<usize, UsbHidError> {
        if self.idle_manager.borrow().is_duplicate(report) {
            self.tick()?;
            Err(UsbHidError::Duplicate)
        } else {
            let data = report.pack().map_err(|e| {
                error!("Error packing BootKeyboardReport: {:?}", e);
                UsbHidError::SerializationError
            })?;
            match self.inner.write_report(&data) {
                Ok(n) => {
                    self.idle_manager.borrow_mut().report_written(*report);
                    Ok(n)
                }
                Err(UsbError::WouldBlock) => Err(UsbHidError::WouldBlock),
                Err(e) => Err(UsbHidError::UsbError(e)),
            }
        }
    }

    pub fn tick(&self) -> Result<(), UsbHidError> {
        let mut idle_manager = self.idle_manager.borrow_mut();
        if !(idle_manager.is_idle_expired()) {
            Ok(())
        } else if let Some(r) = idle_manager.last_report.as_ref() {
            let data = r.pack().map_err(|e| {
                error!("Error packing BootKeyboardReport: {:?}", e);
                UsbHidError::SerializationError
            })?;
            match self.inner.write_report(&data) {
                Ok(n) => {
                    idle_manager.reset_timer();
                    Ok(n)
                }
                Err(UsbError::WouldBlock) => Err(UsbHidError::WouldBlock),
                Err(e) => Err(UsbHidError::UsbError(e)),
            }
            .map(|_| ())
        } else {
            Ok(())
        }
    }

    pub fn read_report(&self) -> usb_device::Result<KeyboardLedsReport> {
        let data = &mut [0];
        match self.inner.read_report(data) {
            Err(e) => Err(e),
            Ok(_) => match KeyboardLedsReport::unpack(data) {
                Ok(r) => Ok(r),
                Err(_) => Err(UsbError::ParseError),
            },
        }
    }

    pub fn default_config(clock: &'a C) -> WrappedInterfaceConfig<'a, Self, &'a C> {
        WrappedInterfaceConfig::new(
            InterfaceBuilder::new(NKRO_BOOT_KEYBOARD_REPORT_DESCRIPTOR)
                .description("NKRO Keyboard")
                .boot_device(InterfaceProtocol::Keyboard)
                .idle_default(Milliseconds(500))
                .unwrap()
                .in_endpoint(UsbPacketSize::Bytes32, Milliseconds(10))
                .unwrap()
                .with_out_endpoint(UsbPacketSize::Bytes8, Milliseconds(100))
                .unwrap()
                .build(),
            clock,
        )
    }
}

impl<'a, B: UsbBus, C: Clock<T = u64>> InterfaceClass<'a> for NKROBootKeyboardInterface<'a, B, C> {
    delegate! {
        to self.inner{
           fn report_descriptor(&self) -> &'a [u8];
           fn id(&self) -> InterfaceNumber;
           fn write_descriptors(&self, writer: &mut DescriptorWriter) -> usb_device::Result<()>;
           fn get_string(&self, index: StringIndex, _lang_id: u16) -> Option<&'static str>;
           fn set_report(&mut self, data: &[u8]) -> usb_device::Result<()>;
           fn get_report(&mut self, data: &mut [u8]) -> usb_device::Result<usize>;
           fn get_report_ack(&mut self) -> usb_device::Result<()>;
           fn get_idle(&self, report_id: u8) -> u8;
           fn set_protocol(&mut self, protocol: HidProtocol);
           fn get_protocol(&self) -> HidProtocol;
        }
    }

    fn reset(&mut self) {
        self.inner.reset();
        self.idle_manager.borrow_mut().reset();
    }
    fn set_idle(&mut self, report_id: u8, value: u8) {
        self.inner.set_idle(report_id, value);
        if report_id == 0 {
            self.idle_manager
                .borrow_mut()
                .set_duration(self.inner.global_idle());
        }
    }
}

impl<'a, B: UsbBus, C: Clock<T = u64>> WrappedInterface<'a, B, &'a C>
    for NKROBootKeyboardInterface<'a, B, C>
{
    fn new(interface: RawInterface<'a, B>, config: &'a C) -> Self {
        let default_idle = interface.global_idle();
        Self {
            inner: interface,
            idle_manager: RefCell::new(IdleManager::new(config, default_idle)),
        }
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

    use crate::device::keyboard::{BootKeyboardReport, KeyboardLedsReport};
    use crate::page::Keyboard;

    #[test]
    fn leds_num_lock() {
        assert_eq!(
            KeyboardLedsReport::unpack(&[1]),
            Ok(KeyboardLedsReport {
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
            KeyboardLedsReport::unpack(&[2]),
            Ok(KeyboardLedsReport {
                num_lock: false,
                caps_lock: true,
                scroll_lock: false,
                compose: false,
                kana: false,
            })
        );

        assert_eq!(
            KeyboardLedsReport {
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
