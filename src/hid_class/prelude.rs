//! The Usb Hid Class Prelude.
//!
//! The purpose of this module is to alleviate imports of structs and enums
//! required to build and instance a [`UsbHidClass`]:
//!
//! ```
//! # #![allow(unused_imports)]
//! use usbd-hid-devices::hid_class::prelude::*;
//! ```

pub use super::{descriptor::InterfaceProtocol, UsbHidClass, UsbHidClassBuilder, UsbPacketSize};
