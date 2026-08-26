use core::{fmt::Write, ptr};

use embassy_executor::Spawner;
use embassy_stm32::bind_interrupts;
use embassy_stm32::peripherals::{PA11, PA12, USB};
use embassy_stm32::usb;
use embassy_stm32::usb::Driver;
use embassy_stm32::Peri;
use embassy_time::{Duration, Timer};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::Builder;
use heapless::String;
use static_cell::StaticCell;

bind_interrupts!(struct Irqs {
    USB_DRD_FS => usb::InterruptHandler<USB>;
});

static CONFIG_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
static BOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
static CONTROL_BUF: StaticCell<[u8; 512]> = StaticCell::new();
static SERIAL_NUMBER: StaticCell<String<24>> = StaticCell::new();

static STATE_CELL: StaticCell<State> = StaticCell::new();
pub struct Serial;

impl Serial {
    pub fn init(
        usb: Peri<'static, USB>,
        dp: Peri<'static, PA12>,
        dm: Peri<'static, PA11>,
        spawner: &Spawner,
    ) {
        let config_desc = CONFIG_DESCRIPTOR.init([0; 256]);
        let bos_desc = BOS_DESCRIPTOR.init([0; 256]);
        let control = CONTROL_BUF.init([0; 512]);

        let driver = Driver::new(usb, Irqs, dp, dm);

        let mut cfg = embassy_usb::Config::new(0xc0de, 0xcafe);
        cfg.manufacturer = Some("RACE UP");
        cfg.product = Some(concat!("USB-", env!("CARGO_PKG_NAME")));
        cfg.serial_number = Some(mk_usb_serial());

        let state: &'static mut State = STATE_CELL.init(State::new());

        let mut builder = Builder::new(driver, cfg, config_desc, bos_desc, &mut [], control);

        let cdc = CdcAcmClass::new(&mut builder, state, 64);
        let usb_dev = builder.build();

        spawner.spawn(usb_driver_task(usb_dev).unwrap());
        spawner.spawn(usb_io_task(cdc).unwrap());
    }
}

#[embassy_executor::task]
pub async fn usb_driver_task(
    mut usb_dev: embassy_usb::UsbDevice<'static, Driver<'static, USB>>,
) -> ! {
    usb_dev.run().await
}

#[embassy_executor::task]
async fn usb_io_task(mut class: CdcAcmClass<'static, Driver<'static, USB>>) {
    class.wait_connection().await;

    let (_, mut rx) = class.split();
    let mut buf = [0u8; 64];
    while rx.read_packet(&mut buf).await.is_ok() {
        Timer::after(Duration::from_micros(20)).await;
    }
}

pub fn mk_usb_serial() -> &'static str {
    let mut serial = String::new();

    let base = 0x1FFF_7A10 as *const u32;
    let w0 = unsafe { ptr::read_volatile(base) };
    let w1 = unsafe { ptr::read_volatile(base.add(1)) };
    let w2 = unsafe { ptr::read_volatile(base.add(2)) };

    write!(serial, "{:08X}{:08X}{:08X}", w0, w1, w2).unwrap();

    SERIAL_NUMBER.init(serial).as_str()
}
