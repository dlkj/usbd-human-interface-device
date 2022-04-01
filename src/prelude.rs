//! The USB HID Prelude.
//!
//! The purpose of this module is to alleviate imports of structs and enums
//! required to instance USB HID classes and interfaces:
//!
//! ```
//! # #![allow(unused_imports)]
//! use usbd_hid_devices::prelude::*;
//! ```

pub use crate::hid_class::UsbHidClass;
pub use crate::hid_class::UsbHidClassBuilder;
pub use crate::UsbHidError;
