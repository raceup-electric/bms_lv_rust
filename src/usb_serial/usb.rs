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

bind_interrupts!(struct Irqs {
    OTG_FS => usb::InterruptHandler<peripherals::USB_OTG_FS>;
});

static mut EP_OUT_BUFFER:     [u8; 512] = [0; 512];
static mut CONFIG_DESCRIPTOR: [u8; 256] = [0; 256];
static mut BOS_DESCRIPTOR:    [u8; 256] = [0; 256];
static mut CONTROL_BUF:       [u8; 256]  = [0; 256];

// SPSC queue storage for incoming bytes
static STATE_CELL: StaticCell<State> = StaticCell::new();
static RX_QUEUE_CELL: StaticCell<Queue<u8, 256>> = StaticCell::new();
static TX_QUEUE_CELL: StaticCell<Queue<u8, 256>> = StaticCell::new();

static mut TX_QUEUE_PTR: *mut Queue<u8, 256> = core::ptr::null_mut();
static mut RX_QUEUE_PTR: *mut Queue<u8, 256> = core::ptr::null_mut();

pub struct Serial;

impl Serial {
    pub fn init(otg_fs: USB_OTG_FS, pa12: PA12, pa11: PA11, spawner: &Spawner) {
        
        let ep_out = &raw mut EP_OUT_BUFFER;
        let config_desc = &raw mut CONFIG_DESCRIPTOR;
        let bos_desc    = &raw mut BOS_DESCRIPTOR;
        let control     = &raw mut CONTROL_BUF;

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
    mut prod: Producer<'static, u8, 256>,
    mut tx_cons: Consumer<'static, u8, 256>,
) {
    let mut rx_buf = [0u8; 256];
    let mut tx_buf = [0u8; 64];

    loop {
        class.wait_connection().await;

        loop {
            // Make sure host is ready to receive data
            if !class.dtr() {
                embassy_time::Timer::after_millis(10).await;
                continue;
            }

            // Receive from host
            match class.read_packet(&mut rx_buf).await {
                Ok(len) => {
                    for &b in &rx_buf[..len] {
                        let _ = prod.enqueue(b);
                    }
                }
                Err(_) => break, // disconnected or error
            }

            embassy_time::Timer::after_micros(20).await;

            let mut tx_len = 0;
            while tx_len < tx_buf.len() {
                match tx_cons.dequeue() {
                    Some(b) => {
                        tx_buf[tx_len] = b;
                        tx_len += 1;
                    }
                    None => break,
                }
            }

            if tx_len > 0 {
                match class.write_packet(&tx_buf[..tx_len]).await {
                    Ok(_) => {}
                    Err(e) => {
                        defmt::warn!("USB write failed: {:?}", e);
                        break;
                    }
                }
            }

            embassy_time::Timer::after_millis(1).await;
        }
    }
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