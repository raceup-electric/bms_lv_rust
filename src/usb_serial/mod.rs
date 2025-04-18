//! Serial USB communication using CDC-ACM class for Embassy-based STM32 applications.
//!
//! This module abstracts the USB serial communication over the CDC-ACM class using the `embassy-usb` crate. It allows
//! for asynchronous USB communication, including sending debug or runtime information to a host PC. The `UsbDevice`
//! struct manages the USB connection, and the `info!` macro is used to send messages over USB.

pub mod serial_usb;

pub use serial_usb::UsbDevice;