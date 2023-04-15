//! The Usb Hid Class Prelude.
//!
//! The purpose of this module is to alleviate imports of structs and enums
//! required to build and instance a [`UsbHidClass`]:
//!
//! ```
//! # #![allow(unused_imports)]
//! use usbd_human_interface_device::hid_class::prelude::*;
//! ```

pub use super::{
    descriptor::HidProtocol, descriptor::InterfaceProtocol, UsbHidClass, UsbHidClassBuilder,
};
pub use crate::interface::managed::{ManagedInterface, ManagedInterfaceConfig};
pub use crate::interface::raw::{RawInterface, RawInterfaceBuilder};
