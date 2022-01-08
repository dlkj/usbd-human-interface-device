//! USB HID devices
#![no_std]

pub mod hid;
pub mod keyboard;
pub mod mouse;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
