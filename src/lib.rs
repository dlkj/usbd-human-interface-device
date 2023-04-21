#![no_std]
#![warn(clippy::pedantic)]
#![warn(clippy::style)]
#![warn(clippy::cargo)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::struct_excessive_bools)]
#![warn(clippy::unwrap_used)]
#![warn(clippy::expect_used)]
#![warn(clippy::use_self)]

//! ```rust, no_run
//! # use core::option::Option;
//! # use core::result::Result;
//! # use core::todo;
//! # use usb_device::bus::PollResult;
//! # use fugit::{ExtU32, MillisDurationU32};
//! use usbd_human_interface_device::page::Keyboard;
//! use usbd_human_interface_device::device::keyboard::{KeyboardLedsReport, NKROBootKeyboardConfig};
//! use usbd_human_interface_device::prelude::*;
//! # use usb_device::class_prelude::*;
//! # use usb_device::prelude::*;
//! # use usb_device::UsbDirection;
//! #
//! # trait InputPin {
//! #     fn is_high(&self) -> Result<bool, core::convert::Infallible>;
//! #     fn is_low(&self) -> Result<bool, core::convert::Infallible>;
//! # }
//! #
//! # struct DummyUsbBus;
//! # impl UsbBus for DummyUsbBus{fn alloc_ep(&mut self, ep_dir: UsbDirection, ep_addr: Option<EndpointAddress>, ep_type: EndpointType, max_packet_size: u16, interval: u8) -> usb_device::Result<EndpointAddress> {
//! #         todo!()
//! #     }
//! #
//! # fn enable(&mut self) {
//! #         todo!()
//! #     }
//! #
//! # fn reset(&self) {
//! #         todo!()
//! #     }
//! #
//! # fn set_device_address(&self, addr: u8) {
//! #         todo!()
//! #     }
//! #
//! # fn write(&self, ep_addr: EndpointAddress, buf: &[u8]) -> usb_device::Result<usize> {
//! #         todo!()
//! #     }
//! #
//! # fn read(&self, ep_addr: EndpointAddress, buf: &mut [u8]) -> usb_device::Result<usize> {
//! #         todo!()
//! #     }
//! #
//! # fn set_stalled(&self, ep_addr: EndpointAddress, stalled: bool) {
//! #         todo!()
//! #     }
//! #
//! # fn is_stalled(&self, ep_addr: EndpointAddress) -> bool {
//! #         todo!()
//! #     }
//! #
//! # fn suspend(&self) {
//! #         todo!()
//! #     }
//! #
//! # fn resume(&self) {
//! #         todo!()
//! #     }
//! #
//! # fn poll(&self) -> PollResult {
//! #         todo!()
//! #     }}
//! #
//! # let usb_bus = DummyUsbBus{};
//! # let pin: &dyn InputPin = todo!();
//! # let update_leds: fn(KeyboardLedsReport) = todo!();
//! #
//! # struct CountDown;
//! #
//! # impl CountDown{
//! #     fn start(&mut self, count: MillisDurationU32){}
//! #     fn wait(&mut self) -> Result<(), ()>{ todo!() }
//! # }
//! #
//! # struct Timer;
//! # impl Timer {
//! #    fn count_down(&self) -> CountDown {
//! #        todo!()
//! #    }
//! # }
//! # let timer: Timer = todo!();
//!
//! let usb_alloc = UsbBusAllocator::new(usb_bus);
//!
//! let mut keyboard = UsbHidClassBuilder::new()
//!     .add_device(
//!         NKROBootKeyboardConfig::default(),
//!     )
//!     .build(&usb_alloc);
//!
//! let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
//!     .manufacturer("usbd-human-interface-device")
//!     .product("NKRO Keyboard")
//!     .serial_number("TEST")
//!     .build();
//!
//! let mut tick_timer = timer.count_down();
//! tick_timer.start(1.millis());
//!
//! loop {
//!     let keys = if pin.is_high().unwrap() {
//!             [Keyboard::A]
//!         } else {
//!             [Keyboard::NoEventIndicated]
//!     };
//!
//!     keyboard.device().write_report(keys).ok();
//!
//!     // tick once per ms/at 1kHz
//!     if tick_timer.wait().is_ok() {
//!         keyboard.tick().unwrap();
//!     }
//!
//!     if usb_dev.poll(&mut [&mut keyboard]) {
//!         match keyboard.device().read_report() {
//!
//!             Ok(l) => {
//!                 update_leds(l);
//!             }
//!             _ => {}
//!
//!         }
//!     }
//! }
//! ```

#![doc = include_str!("../README.md")]

pub(crate) mod fmt;

//Allow the use of std in tests
#[cfg(test)]
extern crate std;

use usb_device::UsbError;

pub mod descriptor;
pub mod device;
pub mod interface;
pub mod page;
pub mod prelude;
pub mod usb_class;

#[derive(Debug)]
pub enum UsbHidError {
    WouldBlock,
    Duplicate,
    UsbError(UsbError),
    SerializationError,
}

impl From<UsbError> for UsbHidError {
    fn from(e: UsbError) -> Self {
        match e {
            UsbError::WouldBlock => Self::WouldBlock,
            _ => Self::UsbError(e),
        }
    }
}

mod private {
    /// Super trait used to mark traits with an exhaustive set of
    /// implementations
    pub trait Sealed {}
}
