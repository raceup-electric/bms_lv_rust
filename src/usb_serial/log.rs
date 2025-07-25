
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

