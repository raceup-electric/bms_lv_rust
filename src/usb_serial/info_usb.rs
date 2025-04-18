use super::serial_usb::UsbDevice;

#[macro_export]
macro_rules! usb_info {
    ($class:expr, $($arg:tt)*) => {{
        use heapless::String;
        use core::fmt::Write;

        // Create a new String buffer (256 bytes maximum)
        let mut buf: String<256> = String::new();

        // Format the arguments into the buffer (similar to `info!` formatting)
        let _ = write!(&mut buf, $($arg)*);

        // Use your `write_usb` function to send the formatted message
        let _ = write_usb($class, buf.as_bytes()).await;
    }};
}