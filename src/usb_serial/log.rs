use core::panic::PanicInfo;
use core::fmt::Write;

use cortex_m::asm;
use defmt::Logger;
use defmt::Encoder;
use crate::Serial;

#[defmt::global_logger]
pub struct UsbDefmt;

/// The `Encoder` holds the defmt wire‐format state machine.
static mut ENCODER: Encoder = Encoder::new();

fn do_write(bytes: &[u8]) {
    Serial::write(bytes);
}

/// Implement the new defmt::Logger trait
/// — see https://defmt.ferrous-systems.com/global-logger
unsafe impl Logger for UsbDefmt {
    fn acquire() {
        unsafe {(&mut *(&raw mut ENCODER)).start_frame(do_write) };
    }

    unsafe fn write(bytes: &[u8]) {
        (&mut *(&raw mut ENCODER)).write(bytes, do_write);
    }

    unsafe fn release() {
        (&mut *(&raw mut ENCODER)).end_frame(do_write);
    }

    unsafe fn flush() {
        while Serial::write_len() != 0 {
            cortex_m::asm::nop();
        }
        do_write(&[]);
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // 1) Format the panic message
    let mut buf = heapless::String::<256>::new();
    if let Some(location) = info.location() {
        let _ = write!(buf, "PANIC at {}:{}\r\n", location.file(), location.line());
    } else {
        let _ = write!(buf, "  {}\r\n", info.message());
    }

    // 2) Enqueue it on our Serial
    Serial::write(buf.as_bytes());

    Serial::flush();

    for _ in 0..10_000_000 {
        cortex_m::asm::nop();
    }
    
    asm::udf();
}

