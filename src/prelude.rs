//! Prelude for using USB HID devices
//!
//! The purpose of this module is to alleviate imports of structs and enums
//! required to instance USB HID devices:
//!
//! ```
//! # #![allow(unused_imports)]
//! use usbd_human_interface_device::prelude::*;
//! ```

pub use crate::usb_class::{UsbHidClass, UsbHidClassBuilder};
pub use crate::UsbHidError;
