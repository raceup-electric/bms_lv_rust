//! USB Serial Communication module using the CDC-ACM class with Embassy and STM32.
//!
//! This module provides asynchronous USB CDC functionality using the `embassy-usb` stack. It is
//! primarily used to send debug or runtime information to a host PC over USB.

// Core and crate imports
use core::cell::Cell;

use defmt::panic;
use embassy_stm32::peripherals::{USB_OTG_FS, PA11, PA12};
use embassy_stm32::usb::Driver;
use embassy_stm32::{bind_interrupts, peripherals, usb};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::CriticalSectionMutex;
use embassy_sync::mutex::Mutex;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::driver::EndpointError;
use embassy_usb::{Builder, Config};
use static_cell::StaticCell;
use heapless::String;

// Interrupt binding for USB OTG FS
bind_interrupts!(struct Irqs {
    OTG_FS => usb::InterruptHandler<peripherals::USB_OTG_FS>;
});

// Static buffers for USB
static STATE: StaticCell<State<'static>> = StaticCell::new();
static CONFIG_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
static BOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();
static EP_OUT_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static MSOS_DESCRIPTOR: StaticCell<[u8; 0]> = StaticCell::new();

/// Static buffer used to pass outgoing messages from other parts of the system to the USB task.
pub static SEND_BUF: CriticalSectionMutex<Cell<Option<String<128>>>> = CriticalSectionMutex::new(Cell::new(None));

/// A USB CDC-ACM device instance with Embassy integration.
pub struct UsbDevice {
    usb_class: CdcAcmClass<'static, Driver<'static, peripherals::USB_OTG_FS>>,
    usb_dev: Option<embassy_usb::UsbDevice<'static, Driver<'static, peripherals::USB_OTG_FS>>>,
    _config: Config<'static>,
}

impl UsbDevice {
    /// Creates a new `UsbDevice` instance with the given USB peripheral and pins.
    ///
    /// # Arguments
    ///
    /// * `peri` - USB peripheral.
    /// * `dp` - USB D+ pin.
    /// * `dm` - USB D- pin.
    pub fn new(peri: USB_OTG_FS, dp: PA12, dm: PA11) -> Self {
        let mut config = embassy_stm32::usb::Config::default();
        config.vbus_detection = false;

        let state = STATE.init(State::new());
        let config_descriptor = CONFIG_DESCRIPTOR.init([0; 256]);
        let bos_descriptor = BOS_DESCRIPTOR.init([0; 256]);
        let control_buf = CONTROL_BUF.init([0; 64]);
        let ep_out_buf = EP_OUT_BUF.init([0; 256]);
        let msos_descriptor = MSOS_DESCRIPTOR.init([]);

        let driver = Driver::new_fs(peri, Irqs, dp, dm, ep_out_buf, config);

        let mut config = Config::new(0xc0de, 0xcafe);
        config.manufacturer = Some("RUSTBELLISSIMO");
        config.product = Some("CRACCOGAYYYYYY");
        config.serial_number = Some("12345678");

        let mut builder = Builder::new(
            driver,
            config,
            config_descriptor,
            bos_descriptor,
            msos_descriptor,
            control_buf,
        );

        let usb_class = CdcAcmClass::new(&mut builder, state, 64);
        let usb_dev = builder.build();

        UsbDevice {
            usb_class,
            usb_dev: Some(usb_dev),
            _config: config,
        }
    }

    /// Asynchronously writes a message over USB if a message is present in `SEND_BUF`.
    ///
    /// # Returns
    /// * `Ok(())` - If the message was successfully sent or there was nothing to send.
    /// * `Err(Disconnected)` - If the USB device is no longer connected.
    pub async fn write_usb(&mut self) -> Result<(), Disconnected> {
        let mut buf: Option<String<128>> = None;

        SEND_BUF.lock(|cell| {
            buf = cell.take();
        });

        if let Some(data) = buf {
            match self.usb_class.write_packet(data.as_bytes()).await {
                Ok(_) => Ok(()),
                Err(_) => Err(Disconnected {}),
            }
        } else {
            Ok(())
        }
    }
}

/// Runs the USB device task. Should be spawned once at startup.
///
/// # Arguments
/// * `usb` - A reference to the shared USB device mutex.
#[embassy_executor::task]
pub async fn usb_run_task(
    usb: &'static Mutex<CriticalSectionRawMutex, UsbDevice>,
) {
    let mut usb_data = usb.lock().await;
    let dev = usb_data.usb_dev.take();
    drop(usb_data);
    if let Some(mut usb_dev) = dev {
        usb_dev.run().await;
    }
}

/// Represents a USB disconnection or endpoint error.
pub struct Disconnected {}

impl From<EndpointError> for Disconnected {
    fn from(val: EndpointError) -> Self {
        match val {
            EndpointError::BufferOverflow => panic!("Buffer overflow"),
            EndpointError::Disabled => Disconnected {},
        }
    }
}

/// Logs formatted strings over USB CDC-ACM.
///
/// This macro takes a format string and arguments similar to `core::format!`, stores the formatted
/// string in a static buffer, and triggers the USB task to send it to the host.
///
/// # Example
///
/// ```rust
/// info!("Battery voltage: {}mV", voltage);
/// ```
///
/// # Notes
///
/// * Messages are saved via a global static buffer `SEND_BUF` to be eventually sent by the `write_usb` task.
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {{
        use core::fmt::Write;

        let mut buf = heapless::String::<128>::new();
        let _ = write!(&mut buf, $($arg)*);

        crate::usb_serial::serial_usb::SEND_BUF.lock(|cell| {
            cell.set(Some(buf));
        });
    }};
}
