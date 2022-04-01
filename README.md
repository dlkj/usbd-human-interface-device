usbd-human-interface-device
===========================

[![Library build](https://github.com/dlkj/usbd-human-interface-device/actions/workflows/lib_build.yml/badge.svg)](https://github.com/dlkj/usbd-human-interface-device/actions/workflows/lib_build.yml)
[![Example build](https://github.com/dlkj/usbd-human-interface-device/actions/workflows/example_build.yml/badge.svg)](https://github.com/dlkj/usbd-human-interface-device/actions/workflows/example_build.yml)
[![Security audit](https://github.com/dlkj/usbd-human-interface-device/actions/workflows/audit.yml/badge.svg)](https://github.com/dlkj/usbd-human-interface-device/actions/workflows/audit.yml)


[![crates.io](https://img.shields.io/crates/v/usbd-human-interface-device.svg)](https://crates.io/crates/usbd-human-interface-device)
[![docs.rs](https://docs.rs/usbd-human-interface-device/badge.svg)](https://docs.rs/usbd-human-interface-device)


Batteries included embedded USB HID library for [usb-device](https://crates.io/crates/usb-device). Includes concrete
Keyboard (boot and NKRO), Mouse and Consumer Control implementations as well as support for building your own HID classes.

This library has been tested on the RP2040 but should work on any platform supported by [usb-device](https://crates.io/crates/usb-device).

Devices created with this library should work with any USB host. It has been tested on Windows, Linux and Android. MacOS
should work but has not been verified.

```rust,no_run
use usbd_human_interface_device::page::Keyboard;
use usbd_human_interface_device::device::keyboard::{KeyboardLedsReport, NKROBootKeyboardInterface};
use usbd_human_interface_device::prelude::*;
use usbd_human_interface_device::prelude::*;


let usb_alloc = UsbBusAllocator::new(usb_bus);

let mut keyboard = UsbHidClassBuilder::new()
    .add_interface(
        NKROBootKeyboardInterface::default_config(&clock),
    )
    .build(&usb_alloc);

let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(0x1209, 0x0001))
    .manufacturer("usbd-human-interface-device")
    .product("NKRO Keyboard")
    .serial_number("TEST")
    .build();

loop {

    let keys = if pin.is_high().unwrap() {
            &[Keyboard::A]
        } else {
            &[Keyboard::NoEventIndicated]
    };
    
    keyboard.interface().write_report(keys).ok();
    keyboard.interface().tick().unwrap();
    
    if usb_dev.poll(&mut [&mut keyboard]) {
        match keyboard.interface().read_report() {

            Ok(l) => {
                update_leds(l);
            }
            _ => {}
        }
    }
}
```

Features
--------

* Keyboard implementations - standard boot compliant keyboard, boot compatible NKRO(N-Key Roll Over) keyboard
* Mouse - standard boot compliant mouse, boot compatible mouse with scroll wheel and pan
* Consumer Control - fixed function media control device, arbitrary consumer control device
* Enums defining the Consumer, Desktop, Game, Keyboard, LED, Simulation and Telephony HID usage pages
* Support for multi-interface devices
* Support for HID idle
* Support for HID protocol changing
* Support for both single and multiple reports

Examples
--------

See [examples](https://github.com/dlkj/usbd-human-interface-device/tree/main/examples/src/bin) for demonstration of how
to use this library on the RP2040 (Raspberry Pi Pico)


Roadmap
-------

* Examples and testing for other microcontroller families
* Testing on MacOS
* Support for host device remote wakeup
* Consumer Control implementation supporting scalar values
* Implementation of common Game and Simulation devices

Contact
-------

https://github.com/dlkj/usbd-human-interface-device/issues

License
-------

Distributed under the MIT License. See [MIT](https://opensource.org/licenses/MIT) for details.


Contributing
------------

Contributions are welcome. Please for the project, create a feature branch and open a pull request

Any contribution submitted for inclusion in the work by you will be licenced under the MIT License without any additional terms or conditions.


Acknowledgements
----------------

This library was inspired by existing rust USB libraries and the following sources of USB information:

* [usb-device](https://crates.io/crates/usb-device)
* [usbd-hid](https://crates.io/crates/usbd-hid)
* [usbd-serial](https://crates.io/crates/usbd-serial)
* [Keyberon](https://crates.io/crates/keyberon)
* [USB Made Simple](https://www.usbmadesimple.co.uk/)
* [USB in a NutShell](https://www.beyondlogic.org/usbnutshell/usb1.shtml)
* [Frank Zhao's USB Descriptor and Request Parser](https://eleccelerator.com/usbdescreqparser/)
* [Adafruit](https://learn.adafruit.com/custom-hid-devices-in-circuitpython/n-key-rollover-nkro-hid-device)
* [USBlyzer](http://www.usblyzer.com/)