//! HID descriptor constants and enumerations
use num_enum::{IntoPrimitive, TryFromPrimitive};
use packed_struct::prelude::*;

pub(crate) const USB_CLASS_HID: u8 = 0x03;
pub(crate) const SPEC_VERSION_1_11: u16 = 0x0111; //1.11 in BCD
pub(crate) const COUNTRY_CODE_NOT_SUPPORTED: u8 = 0x0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Ord, PartialOrd, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum InterfaceProtocol {
    None = 0x00,
    Keyboard = 0x01,
    Mouse = 0x02,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PrimitiveEnum, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub(crate) enum DescriptorType {
    Hid = 0x21,
    Report = 0x22,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub(crate) enum InterfaceSubClass {
    None = 0x00,
    Boot = 0x01,
}

impl From<InterfaceProtocol> for InterfaceSubClass {
    fn from(protocol: InterfaceProtocol) -> Self {
        if protocol == InterfaceProtocol::None {
            Self::None
        } else {
            Self::Boot
        }
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum HidProtocol {
    Boot = 0x00,
    Report = 0x01,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub(crate) enum HidRequest {
    GetReport = 0x01,
    GetIdle = 0x02,
    GetProtocol = 0x03,
    SetReport = 0x09,
    SetIdle = 0x0A,
    SetProtocol = 0x0B,
}
