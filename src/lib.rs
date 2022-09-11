//! Batteries included embedded USB HID library for [usb-device](https://crates.io/crates/usb-device). Includes concrete
//! Keyboard (boot and NKRO), Mouse and Consumer Control implementations as well as support for building your own HID classes.
//!
//! This library has been tested on the RP2040 but should work on any platform supported by [usb-device](https://crates.io/crates/usb-device).
//!
//! Devices created with this library should work with any USB host. It has been tested on Windows, Linux and Android. MacOS
//! should work but has not been verified.
//!
//! ```rust, no_run
//! # use core::option::Option;
//! # use core::result::Result;
//! # use core::todo;
//! # use embedded_time::{Clock, Instant};
//! # use embedded_time::clock::Error;
//! # use embedded_time::duration::{Fraction, Milliseconds};
//! # use usb_device::bus::PollResult;
//! use usbd_human_interface_device::page::Keyboard;
//! use usbd_human_interface_device::device::keyboard::{KeyboardLedsReport, NKROBootKeyboardInterface};
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
//! #     fn start(&mut self, count: Milliseconds){}
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
//! #
//! let usb_alloc = UsbBusAllocator::new(usb_bus);
//!
//! let mut keyboard = UsbHidClassBuilder::new()
//!     .add_interface(
//!         NKROBootKeyboardInterface::default_config(),
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
//! tick_timer.start(Milliseconds(1u32));
//!
//! loop {
//!     let keys = if pin.is_high().unwrap() {
//!             &[Keyboard::A]
//!         } else {
//!             &[Keyboard::NoEventIndicated]
//!     };
//!     
//!     keyboard.interface().write_report(keys).ok();
//!
//!     //tick once per ms
//!     if tick_timer.wait().is_ok() {
//!         keyboard.interface().tick().unwrap();
//!     }
//!     
//!     if usb_dev.poll(&mut [&mut keyboard]) {
//!         match keyboard.interface().read_report() {
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
//!
//! Features
//! --------
//!
//! * Keyboard implementations - standard boot compliant keyboard, boot compatible NKRO(N-Key Roll Over) keyboard
//! * Mouse - standard boot compliant mouse, boot compatible mouse with scroll wheel and pan
//! * Consumer Control - fixed function media control device, arbitrary consumer control device
//! * Enums defining the Consumer, Desktop, Game, Keyboard, LED, Simulation and Telephony HID usage pages
//! * Support for multi-interface devices
//! * Support for HID idle
//! * Support for HID protocol changing
//! * Support for both single and multiple reports
//!
//! Examples
//! --------
//!
//! See [examples](https://github.com/dlkj/usbd-human-interface-device/tree/main/examples/src/bin) for demonstration of how
//! to use this library on the RP2040 (Raspberry Pi Pico)!

#![no_std]

//Allow the use of std in tests
#[cfg(test)]
#[macro_use]
extern crate std;

use usb_device::UsbError;

pub mod device;
pub mod hid_class;
pub mod interface;
pub mod page;
pub mod prelude;

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
            UsbError::WouldBlock => UsbHidError::WouldBlock,
            _ => UsbHidError::UsbError(e),
        }
    }
}
