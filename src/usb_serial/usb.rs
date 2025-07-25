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

static EP_OUT_BUFFER: StaticCell<[u8; 256]> = StaticCell::new();
static CONFIG_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
static BOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
static CONTROL_BUF: StaticCell<[u8; 512]>  = StaticCell::new();

// SPSC queue storage for incoming bytes
static STATE_CELL: StaticCell<State> = StaticCell::new();
static RX_QUEUE_CELL: StaticCell<Queue<u8, 256>> = StaticCell::new();
static TX_QUEUE_CELL: StaticCell<Queue<u8, 256>> = StaticCell::new();

static mut TX_QUEUE_PTR: *mut Queue<u8, 256> = core::ptr::null_mut();
static mut RX_QUEUE_PTR: *mut Queue<u8, 256> = core::ptr::null_mut();

pub struct Serial;

#[allow(unused)]
impl Serial {
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
        
        let cdc = CdcAcmClass::new(&mut builder, state, 64);
        let usb_dev = builder.build();

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

    pub fn available() -> usize {
        unsafe { (*RX_QUEUE_PTR).len() }
    }

    pub fn read() -> Option<u8> {
        unsafe { (*RX_QUEUE_PTR).dequeue() }
    }

    pub fn write(buf: &[u8]) {
        for &b in buf {
            unsafe { let _ = (*TX_QUEUE_PTR).enqueue(b); }
        }
    }

    pub fn write_nl(buf: &[u8]) {
        for &b in buf {
            unsafe { let _ = (*TX_QUEUE_PTR).enqueue(b); }
        }

        unsafe { let _ = (*TX_QUEUE_PTR).enqueue('\r' as u8); }
        unsafe { let _ = (*TX_QUEUE_PTR).enqueue('\n' as u8); }
    }

    pub fn write_len() -> usize{
        unsafe {(*TX_QUEUE_PTR).len()}
    }
}


#[embassy_executor::task]
pub async fn usb_driver_task(
    mut usb_dev: embassy_usb::UsbDevice<'static, Driver<'static, USB_OTG_FS>>,
) -> ! {
    usb_dev.run().await
}

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
            if !tx.dtr() {
                embassy_time::Timer::after_millis(2).await;
                continue;
            }

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
                    // Send ZLP
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

pub fn mk_usb_serial() -> &'static str {
    // 3×32‑bit words → 24 hex digits
    static mut SERIAL_BUF: String<24> = String::new();

    let buf: &mut String<24> = unsafe{&mut *(&raw mut SERIAL_BUF)};
    buf.clear();

    let base = 0x1FFF_7A10 as *const u32;
    let w0 = unsafe { ptr::read_volatile(base) };
    let w1 = unsafe { ptr::read_volatile(base.add(1)) };
    let w2 = unsafe { ptr::read_volatile(base.add(2)) };

    write!(buf, "{:08X}{:08X}{:08X}", w0, w1, w2).unwrap();

    buf.as_str()
}
