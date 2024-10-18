# Examples

- keyboard_boot - keyboard implementing the HID boot specification

The examples target the Raspberry Pi Pico2/RP235x but can be ported to other boards.

## Installation of development dependencies

```shell
$ rustup target add thumbv8m.main-none-eabihf
$ rustup target add riscv32imac-unknown-none-elf
```

You can also 'run' an example, which invoke Raspberry Pi's picotool to copy it to an RP235x in USB Bootloader mode.
You should install that if you don't have it already, from https://github.com/raspberrypi/pico-sdk-tools/releases.

These examples are based off the [rp235x-hal-examples](https://github.com/rp-rs/rp-hal/tree/main/rp235x-hal-examples).
See the project README for more detailed instructions.

## Running

```shell
cargo run --release --bin {example name}
```
