use packed_struct::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PrimitiveEnum)]
#[repr(u8)]
pub enum HidRequest {
    GetReport = 0x01,
    GetIdle = 0x02,
    GetProtocol = 0x03,
    SetReport = 0x09,
    SetIdle = 0x0A,
    SetProtocol = 0x0B,
}
