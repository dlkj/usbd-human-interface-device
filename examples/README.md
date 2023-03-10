# Examples

* composite - composite keyboard, mouse and consumer control device
* composite_irq - composite keyboard, mouse and consumer control device using irqs rather than polling
* consumer_fixed - consumer control device with fixed media control functions
* consumer_multiple - consumer control device with arbitary functions
* joystick - two axis joystick with eight buttons
* keyboard_boot - keyboard implementing the HID boot specification
* keyboard_custom - example of building a custom device
* keyboard_nkro - keybard implementing n-key-roll-over and HID boot
* mouse_boot - mouse implementing the HID boot specification
* mouse_wheel - mouse implementing pan and scroll wheels

The examples target the Raspbery Pi Pico but can be ported to other boards by changing the board support package
import and cargo dependency.

## Installation of development dependencies

```shell
rustup target install thumbv6m-none-eabi
cargo install flip-link
# This is the suggested default 'runner'
cargo install probe-run
# If you want to use elf2uf2-rs instead of probe-run, instead do...
cargo install elf2uf2-rs
```

These examples are based off the [rp2040-project-template](https://github.com/rp-rs/rp2040-project-template).
See the project README for details of alternative runners.

## Running

```shell
cargo run --release --bin {example name}
```