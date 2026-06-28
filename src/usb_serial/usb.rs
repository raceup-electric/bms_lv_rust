//! USB-CDC (virtual serial) transport.
//!
//! Brings up the USB OTG FS peripheral as a CDC-ACM device and runs two async
//! tasks: one driving the USB stack, one shuttling bytes between the endpoints
//! and a pair of SPSC byte queues. The static [`Serial`] facade lets any part
//! of the firmware (including the defmt logger) push/pull bytes without holding
//! the device, by talking to the queues through raw pointers.
//!
//! The device serial number is derived from the STM32 unique-ID registers.

use embassy_time::{Duration, Timer};
use embassy_usb::Builder;
use embassy_stm32::usb::Driver;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_stm32::peripherals;
use embassy_stm32::peripherals::{USB_OTG_FS, PA11, PA12};
use embassy_stm32::bind_interrupts;
use embassy_stm32::usb;
use static_cell::StaticCell;
use embassy_executor::Spawner;
use heapless::String;
use heapless::spsc::{Queue, Producer, Consumer};
use core::{ptr, fmt::Write};
use embassy_futures::join::join;

bind_interrupts!(struct Irqs {
    OTG_FS => usb::InterruptHandler<peripherals::USB_OTG_FS>;
});

// Statically allocated buffers required by the USB stack (one-time init).
static EP_OUT_BUFFER: StaticCell<[u8; 256]> = StaticCell::new();
static CONFIG_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
static BOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
static CONTROL_BUF: StaticCell<[u8; 512]>  = StaticCell::new();

// SPSC queue storage for incoming/outgoing bytes.
static STATE_CELL: StaticCell<State> = StaticCell::new();
static RX_QUEUE_CELL: StaticCell<Queue<u8, 256>> = StaticCell::new();
static TX_QUEUE_CELL: StaticCell<Queue<u8, 256>> = StaticCell::new();

// Raw pointers to the queues so the static `Serial` facade can reach them.
static mut TX_QUEUE_PTR: *mut Queue<u8, 256> = core::ptr::null_mut();
static mut RX_QUEUE_PTR: *mut Queue<u8, 256> = core::ptr::null_mut();

/// Zero-sized facade over the USB-CDC byte queues.
pub struct Serial;

#[allow(unused)]
impl Serial {
    /// Initialises the USB-CDC device and spawns its driver/IO tasks.
    ///
    /// Reuses the CAN1 pins (`PA11`/`PA12`) as the USB D-/D+ lines. Must be
    /// called once during startup.
    pub fn init(otg_fs: USB_OTG_FS, pa12: PA12, pa11: PA11, spawner: &Spawner) {

        let ep_out  = EP_OUT_BUFFER.init([0; 256]);
        let config_desc = CONFIG_DESCRIPTOR.init([0; 256]);
        let bos_desc    = BOS_DESCRIPTOR.init([0; 256]);
        let control     = CONTROL_BUF.init([0; 512]);

        let mut config = embassy_stm32::usb::Config::default();

        // Do not enable vbus_detection. This is a safe default that works in all boards.
        // However, if your USB device is self-powered (can stay powered on if USB is unplugged), you need
        // to enable vbus_detection to comply with the USB spec. If you enable it, the board
        // has to support it or USB won't work at all. See docs on `vbus_detection` for details.
        config.vbus_detection = false;

        let driver = Driver::new_fs(otg_fs, Irqs, pa12, pa11, unsafe{&mut *ep_out}, config);

        // USB device descriptors (VID/PID + strings).
        let mut config = embassy_usb::Config::new(0xc0de, 0xcafe);
        config.manufacturer = Some("RACE UP");
        config.product = Some(concat!("USB-", env!("CARGO_PKG_NAME")));
        config.serial_number = Some(mk_usb_serial());

        let state: &'static mut State = StaticCell::init(&STATE_CELL, State::new());

        let mut builder = Builder::new(
            driver,
            config,
            unsafe{&mut *config_desc},
            unsafe{&mut *bos_desc},
            &mut [], // no msos descriptors
            unsafe{&mut *control},
        );

        // CDC-ACM class with a 64-byte max packet size.
        let cdc = CdcAcmClass::new(&mut builder, state, 64);
        let usb_dev = builder.build();

        // Initialise the RX/TX queues and publish their pointers.
        let rxq: &'static mut _ = StaticCell::init(&RX_QUEUE_CELL, Queue::new());
        let txq: &'static mut _ = StaticCell::init(&TX_QUEUE_CELL, Queue::new());
        unsafe {
            RX_QUEUE_PTR = rxq as *mut _;
            TX_QUEUE_PTR = txq as *mut _;
        }
        let (rx_prod,   _rx_cons)  = rxq.split();
        let (_tx_prod,  tx_cons)  = txq.split();

        spawner.spawn(usb_driver_task(usb_dev)).unwrap();
        spawner.spawn(usb_io_task(cdc, rx_prod, tx_cons)).unwrap();
    }

    /// Number of bytes waiting in the receive queue.
    pub fn available() -> usize {
        unsafe { (*RX_QUEUE_PTR).len() }
    }

    /// Dequeues one received byte, if any.
    pub fn read() -> Option<u8> {
        unsafe { (*RX_QUEUE_PTR).dequeue() }
    }

    /// Awaits and returns one CR/LF-terminated line of up to `N` characters.
    pub async fn read_line<const N: usize>() -> String<N> {
        let mut buf: String<N> = String::new();
        loop {
            if let Some(b) = Self::read() {
                match b {
                    b'\r' => {}
                    b'\n' => {
                        break;
                    }
                    _ => {
                        let _ = buf.push(b as char);
                    }
                }
            } else {
                // no data
                Timer::after(Duration::from_millis(5)).await;
            }
        }
        buf
    }


    /// Enqueues bytes for transmission.
    pub fn write(buf: &[u8]) {
        for &b in buf {
            unsafe { let _ = (*TX_QUEUE_PTR).enqueue(b); }
        }
    }

    /// Enqueues bytes followed by CR/LF.
    pub fn write_nl(buf: &[u8]) {
        for &b in buf {
            unsafe { let _ = (*TX_QUEUE_PTR).enqueue(b); }
        }

        unsafe { let _ = (*TX_QUEUE_PTR).enqueue('\r' as u8); }
        unsafe { let _ = (*TX_QUEUE_PTR).enqueue('\n' as u8); }
    }

    /// Number of bytes pending in the transmit queue.
    pub fn write_len() -> usize{
        unsafe {(*TX_QUEUE_PTR).len()}
    }

    /// No-op flush (the IO task drains the queue asynchronously).
    pub fn flush() {

    }
}


/// Runs the USB device state machine for the lifetime of the program.
#[embassy_executor::task]
pub async fn usb_driver_task(
    mut usb_dev: embassy_usb::UsbDevice<'static, Driver<'static, USB_OTG_FS>>,
) -> ! {
    usb_dev.run().await
}

/// Bridges the CDC endpoints and the SPSC queues.
///
/// After the host opens the port, a reader pumps received packets into the RX
/// queue and a writer drains the TX queue out to the host (sending a ZLP after
/// a full 64-byte packet). The two halves run concurrently via `join`.
#[embassy_executor::task]
async fn usb_io_task(
    mut class: CdcAcmClass<'static, Driver<'static, USB_OTG_FS>>,
    mut rx_prod: Producer<'static, u8, 256>,
    mut tx_cons: Consumer<'static, u8, 256>,
) {
    // Wait until the host opens the port
    class.wait_connection().await;

    // Split into a sender (IN endpoint) and receiver (OUT endpoint)
    let (mut tx, mut rx) = class.split();

    // Reader task
    let reader = async {
        let mut buf = [0u8; 64];
        loop {
            match rx.read_packet(&mut buf).await {
                Ok(len) => {
                    for &b in &buf[..len] {
                        let _ = rx_prod.enqueue(b);
                    }
                }
                Err(_) => break, // host disconnected
            }
            embassy_time::Timer::after_micros(20).await;
        }
    };

    // Writer task
    let writer = async {
        let mut buf = [0u8; 64];
        loop {
            // Only transmit when the host has asserted DTR.
            if !tx.dtr() {
                embassy_time::Timer::after_millis(2).await;
                continue;
            }

            // Pull up to one packet worth of bytes from the queue.
            let mut n = 0;
            while n < buf.len() {
                match tx_cons.dequeue() {
                    Some(b) => {
                        buf[n] = b;
                        n += 1;
                    }
                    None => break,
                }
            }

            if n > 0 {
                if n == 64 {
                    let _ = tx.write_packet(&buf).await;
                    // Send ZLP to terminate a full-size transfer.
                    let _ = tx.write_packet(&[]).await;
                } else {
                    let _ = tx.write_packet(&buf[..n]).await;
                }
            } else {
                embassy_time::Timer::after_micros(20).await;
            }
        }
    };

    join(reader, writer).await;
}

/// Builds the USB serial-number string from the STM32 96-bit unique ID.
pub fn mk_usb_serial() -> &'static str {
    // 3×32-bit words -> 24 hex digits
    static mut SERIAL_BUF: String<24> = String::new();

    let buf: &mut String<24> = unsafe{&mut *(&raw mut SERIAL_BUF)};
    buf.clear();

    // STM32F405 unique-ID base address.
    let base = 0x1FFF_7A10 as *const u32;
    let w0 = unsafe { ptr::read_volatile(base) };
    let w1 = unsafe { ptr::read_volatile(base.add(1)) };
    let w2 = unsafe { ptr::read_volatile(base.add(2)) };

    write!(buf, "{:08X}{:08X}{:08X}", w0, w1, w2).unwrap();

    buf.as_str()
}
