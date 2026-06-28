//! defmt global logger over USB-CDC + panic handler.
//!
//! Routes [`defmt`] frames into the [`Serial`] transmit queue so log macros
//! (`info!`, `error!`, …) appear on the USB virtual serial port, and installs a
//! panic handler that prints the panic location/message before halting.

use core::fmt::Write;
use core::panic::PanicInfo;

use crate::Serial;
use cortex_m::asm;
use defmt::Encoder;
use defmt::Logger;

/// Zero-sized type registered as defmt's global logger.
#[defmt::global_logger]
pub struct UsbDefmt;

/// The `Encoder` holds the defmt wire-format state machine.
static mut ENCODER: Encoder = Encoder::new();

/// Sink callback: pushes encoded bytes into the USB serial queue.
fn do_write(bytes: &[u8]) {
    Serial::write(bytes);
}

/// Implement the new defmt::Logger trait
/// — see <https://defmt.ferrous-systems.com/global-logger>
unsafe impl Logger for UsbDefmt {
    /// Begins a log frame.
    fn acquire() {
        unsafe { (&mut *(&raw mut ENCODER)).start_frame(do_write) };
    }

    /// Encodes and writes a chunk of the current frame.
    unsafe fn write(bytes: &[u8]) {
        (&mut *(&raw mut ENCODER)).write(bytes, do_write);
    }

    /// Ends the current log frame.
    unsafe fn release() {
        (&mut *(&raw mut ENCODER)).end_frame(do_write);
    }

    /// Blocks until the transmit queue has drained.
    unsafe fn flush() {
        while Serial::write_len() != 0 {
            cortex_m::asm::nop();
        }
        do_write(&[]);
    }
}

/// Panic handler: reports the location/message over serial, then halts.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // 1) Format the panic message
    let mut buf = heapless::String::<256>::new();
    if let Some(location) = info.location() {
        let _ = write!(buf, "PANIC at {}:{}\r\n", location.file(), location.line());
    } else {
        let _ = write!(buf, "  {}\r\n", info.message());
    }

    // 2) Enqueue it on our Serial and flush.
    Serial::write(buf.as_bytes());

    Serial::flush();

    // 3) Give the USB IO task time to drain before halting.
    for _ in 0..10_000_000 {
        cortex_m::asm::nop();
    }

    asm::udf();
}
