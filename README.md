[![Library build](https://github.com/dlkj/usbd-human-interface-device/actions/workflows/lib_build.yml/badge.svg)](https://github.com/dlkj/usbd-human-interface-device/actions/workflows/lib_build.yml)
[![Example build](https://github.com/dlkj/usbd-human-interface-device/actions/workflows/example_build.yml/badge.svg)](https://github.com/dlkj/usbd-human-interface-device/actions/workflows/example_build.yml)
[![Security audit](https://github.com/dlkj/usbd-human-interface-device/actions/workflows/audit.yml/badge.svg)](https://github.com/dlkj/usbd-human-interface-device/actions/workflows/audit.yml)

[![crates.io](https://img.shields.io/crates/v/usbd-human-interface-device.svg)](https://crates.io/crates/usbd-human-interface-device)
[![docs.rs](https://docs.rs/usbd-human-interface-device/badge.svg)](https://docs.rs/usbd-human-interface-device)

Batteries included embedded USB HID library for [`usb-device`](https://crates.io/crates/usb-device).
Includes Keyboard (boot and NKRO), Mouse, Joystick and Consumer Control implementations as well as
support for building your own HID classes.

Tested on the RP2040, but should work on any platform supported by
[`usb-device`](https://crates.io/crates/usb-device).

Devices created with this library should work with any USB host. Tested on Windows,
Linux, MacOS and Android.

**Note:** Managed interfaces that support HID idle, such as
[`NKROBootKeyboardInterface`](https://docs.rs/usbd-human-interface-device/latest/usbd_human_interface_device/device/keyboard/struct.NKROBootKeyboardInterface.html)
and [`BootKeyboardInterface`](https://docs.rs/usbd-human-interface-device/latest/usbd_human_interface_device/device/keyboard/struct.BootKeyboardInterface.html),
require the `UsbHidClass::tick()` method calling every 1ms.

## Features

- Keyboard - boot compliant keyboard, boot compliant NKRO(N-Key Roll Over) keyboard
- Mouse - boot compliant mouse, boot compliant mouse with scroll wheel and pan
- Joystick - two axis joystick with eight buttons
- Consumer Control - Media control device, generic consumer control device
- Enums for the Consumer, Desktop, Game, Keyboard, LED, Simulation and Telephony HID usage pages
- Support for multi-interface devices
- Support for HID idle and HID protocol changing
- Support for both single and multi report interfaces
- Compatible with [RTIC](https://rtic.rs)

## Examples

See [examples](https://github.com/dlkj/usbd-human-interface-device/tree/main/examples) for
demonstrations of how to use this library on the RP2040 (Raspberry Pi Pico)

## Road map

- Examples and testing for other micro-controllers such as the SAM D2x family.
- Support for host device remote wakeup

## Contact

<https://github.com/dlkj/usbd-human-interface-device/issues>

## License

Distributed under the MIT License, see [`LICENSE`](https://github.com/dlkj/usbd-human-interface-device/tree/main/LICENSE).

## Contributing

Contributions are welcome via pull requests

## Acknowledgements

This library was inspired by existing rust USB libraries and the following sources of USB information:

- [usb-device](https://crates.io/crates/usb-device)
- [usbd-hid](https://crates.io/crates/usbd-hid)
- [usbd-serial](https://crates.io/crates/usbd-serial)
- [Keyberon](https://crates.io/crates/keyberon)
- [USB Made Simple](https://www.usbmadesimple.co.uk/)
- [USB in a NutShell](https://www.beyondlogic.org/usbnutshell/usb1.shtml)
- [Frank Zhao's USB Descriptor and Request Parser](https://eleccelerator.com/usbdescreqparser/)
- [Adafruit](https://learn.adafruit.com/custom-hid-devices-in-circuitpython/n-key-rollover-nkro-hid-device)
- [USBlyzer](http://www.usblyzer.com/)
