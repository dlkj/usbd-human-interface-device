use log::error;
use packed_struct::prelude::*;
use usb_device::class_prelude::*;
use usb_device::Result;

pub const USB_CLASS_HID: u8 = 0x03;
pub const SPEC_VERSION_1_11: u16 = 0x0111; //1.11 in BCD
pub const COUNTRY_CODE_NOT_SUPPORTED: u8 = 0x0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum InterfaceProtocol {
    None = 0x00,
    Keyboard = 0x01,
    Mouse = 0x02,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PrimitiveEnum)]
#[repr(u8)]
pub enum DescriptorType {
    Hid = 0x21,
    Report = 0x22,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum InterfaceSubClass {
    None = 0x00,
    Boot = 0x01,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PrimitiveEnum)]
#[repr(u8)]
pub enum HidProtocol {
    Boot = 0x00,
    Report = 0x01,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PackedStruct)]
#[packed_struct(endian = "lsb", size_bytes = 7)]
pub struct HidDescriptorBody {
    bcd_hid: u16,
    country_code: u8,
    num_descriptors: u8,
    #[packed_field(ty = "enum", size_bytes = "1")]
    descriptor_type: DescriptorType,
    descriptor_length: u16,
}

pub fn hid_descriptor(class_descriptor_len: usize) -> Result<[u8; 7]> {
    if class_descriptor_len > u16::MAX as usize {
        error!(
            "Report descriptor length {:X} too long to fit in u16",
            class_descriptor_len
        );
        Err(UsbError::InvalidState)
    } else {
        Ok(HidDescriptorBody {
            bcd_hid: SPEC_VERSION_1_11,
            country_code: COUNTRY_CODE_NOT_SUPPORTED,
            num_descriptors: 1,
            descriptor_type: DescriptorType::Report,
            descriptor_length: class_descriptor_len as u16,
        }
        .pack()
        .map_err(|e| {
            error!("Failed to pack HidDescriptor {}", e);
            UsbError::InvalidState
        })?)
    }
}
