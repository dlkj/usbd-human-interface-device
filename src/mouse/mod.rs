//!Implements HID mouse devices
pub mod descriptors;

use usb_device::Result;

/// HIDMouse provides an interface to send mouse movement and button presses to
/// the host device
pub trait HIDMouse {
    /// Writes an input report given representing the update to the mouse state
    /// to the host system
    fn write_mouse(&self, buttons: u8, x: i8, y: i8) -> Result<()>;
}
