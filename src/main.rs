#![no_std]
#![no_main]

//! # BMS Low Voltage System
//! This is the main module for the STM32F405-based BMS using Embassy async runtime.

use embassy_executor::Spawner;
use embassy_stm32::{gpio::{Level, Output, Speed}, time::Hertz, Config};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

/// Internal modules
mod types;
mod can_management;
pub mod usb_serial;
mod ltc_management;

use types::{CanMsg, BMS, TEMPERATURES, VOLTAGES};
use can_management::{can_operation, CanController};
use ltc_management::{ltc6811::MODE, SpiDevice, LTC6811};
use usb_serial::{serial_usb::usb_run_task, UsbDevice};

/// Shared peripherals (mutex protected)
static USB: StaticCell<Mutex<CriticalSectionRawMutex, UsbDevice>> = StaticCell::new();
static BMS: StaticCell<Mutex<CriticalSectionRawMutex, BMS>> = StaticCell::new();
static ERR_CHECK: StaticCell<Mutex<CriticalSectionRawMutex, Output>> = StaticCell::new();
static CAN: StaticCell<Mutex<CriticalSectionRawMutex, CanController>> = StaticCell::new();
static SPI: StaticCell<Mutex<CriticalSectionRawMutex, SpiDevice>> = StaticCell::new();
static LTC: StaticCell<Mutex<CriticalSectionRawMutex, LTC6811>> = StaticCell::new();

/// Main entry point for the embedded async runtime
#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // Clock setup
    let mut config = Config::default();
    {
        use embassy_stm32::rcc::*;
        config.rcc.hse = Some(Hse {
            freq: Hertz(12_000_000),
            mode: HseMode::Oscillator,
        });

        config.rcc.pll_src = PllSource::HSE;
        config.rcc.pll = Some(Pll {
            prediv: PllPreDiv::DIV6,
            mul: PllMul::MUL168,
            divp: Some(PllPDiv::DIV2),
            divq: Some(PllQDiv::DIV7),
            divr: None,
        });

        config.rcc.ahb_pre = AHBPrescaler::DIV1;
        config.rcc.apb1_pre = APBPrescaler::DIV4;
        config.rcc.apb2_pre = APBPrescaler::DIV2;

        config.rcc.sys = Sysclk::PLL1_P;
        config.rcc.mux.clk48sel = mux::Clk48sel::PLL1_Q;
    }

    let p = embassy_stm32::init(config);

    // USB setup
    let usb = UsbDevice::new(p.USB_OTG_FS, p.PA12, p.PA11);
    let usb_mutex = Mutex::new(usb);
    let usb = StaticCell::init(&USB, usb_mutex);

    spawner.spawn(usb_run_task(usb)).unwrap();
    spawner.spawn(send_usb(usb)).unwrap();

    // Error pin setup
    let err_check = Output::new(p.PA2, Level::Low, Speed::High);
    let err_check_mutex = Mutex::new(err_check);
    let err_check = StaticCell::init(&ERR_CHECK, err_check_mutex);

    // BMS state setup
    let bms = setup_bms();
    let bms_mutex = Mutex::new(bms);
    let bms = StaticCell::init(&BMS, bms_mutex);

    spawner.spawn(check_err_check(bms, err_check)).unwrap();

    // CAN setup
    let can = CanController::new_can2(p.CAN2, p.PB12, p.PB13, 500_000, p.CAN1, p.PB8, p.PB9).await;
    let can_mutex = Mutex::new(can);
    let can = StaticCell::init(&CAN, can_mutex);
    spawner.spawn(send_can(bms, can)).unwrap();

    // SPI and LTC6811 setup
    let spi: SpiDevice<'static> = SpiDevice::new(p.SPI1, p.PA5, p.PA7, p.PA6, p.PA4, p.DMA2_CH3, p.DMA2_CH0).await;
    let spi_mutex = Mutex::new(spi);
    let spi = StaticCell::init(&SPI, spi_mutex);

    let mut ltc = LTC6811::new(spi, bms).await;
    match ltc.init().await {
        Ok(_) => info!("LTC6811 initialized successfully"),
        Err(_) => defmt::error!("Failed to initialize LTC6811"),
    }

    let ltc_mutex = Mutex::new(ltc);
    let ltc = StaticCell::init(&LTC, ltc_mutex);
    spawner.spawn(ltc_function(bms, ltc)).unwrap();

    spawner.spawn(read_can(ltc, can)).unwrap();

    loop {
        embassy_time::Timer::after_millis(10).await;
    }
}

/// Setup the BMS state structure
fn setup_bms() -> BMS {
    let bms = BMS::new();
    bms
}

/// Task responsible for sending CAN messages
#[embassy_executor::task]
async fn send_can(
    bms: &'static Mutex<CriticalSectionRawMutex, BMS>, 
    can: &'static Mutex<CriticalSectionRawMutex, CanController<'static>>,
) {
    loop {
        let bms_data = bms.lock().await;
        let mut can_data = can.lock().await;
        can_operation(&bms_data, &mut can_data).await;
        drop(can_data);
        drop(bms_data);
        embassy_time::Timer::after_millis(200).await;
    }
}

/// Task responsible for reading CAN messages and interacting with the LTC6811
#[embassy_executor::task]
async fn read_can(
    ltc: &'static Mutex<CriticalSectionRawMutex, LTC6811>,
    can: &'static Mutex<CriticalSectionRawMutex, CanController<'static>>
) {
    let mut time_now = embassy_time::Instant::now().as_millis();
    loop {
        let mut can_data = can.lock().await;
        match can_data.read().await {
            Ok(frame) => {
                info!("In: {}", frame.byte(0));
                time_now = embassy_time::Instant::now().as_millis();
                let mut ltc_data = ltc.lock().await;
                ltc_data.set_mode(MODE::NORMAL);
                drop(ltc_data);
            }
            Err(_) => {
                info!("No messages");
                if (embassy_time::Instant::now().as_millis() - time_now) > 10000 {
                    // Sleep mode commented out
                }
            }
        }
        drop(can_data);
        embassy_time::Timer::after_millis(10).await;
    }
}

/// Checks if any cell voltage is out of range and toggles an error pin
#[embassy_executor::task]
async fn check_err_check(
    bms: &'static Mutex<CriticalSectionRawMutex, BMS>, 
    err_check: &'static Mutex<CriticalSectionRawMutex, Output<'static>>
) {
    loop {
        let bms_data = bms.lock().await;
        let mut err_check_close = true;

        for cell in &bms_data.cell_volts {
            if cell < &VOLTAGES::MINVOLTAGE.as_raw() || cell > &VOLTAGES::MAXVOLTAGE.as_raw() {
                err_check_close = false;
                break;
            }
        }

        if bms_data.temp() < TEMPERATURES::MINTEMP.as_raw() || bms_data.temp() > TEMPERATURES::MAXTEMP.as_raw() {
            err_check_close = false;
        }
        drop(bms_data);

        let mut err_check_data = err_check.lock().await;
        if err_check_close {
            err_check_data.set_high();
        } else {
            err_check_data.set_low();
        }
        drop(err_check_data);

        embassy_time::Timer::after_millis(100).await;
    }
}

/// Task that updates the LTC6811 and logs voltage statistics
#[embassy_executor::task]
async fn ltc_function(
    bms: &'static Mutex<CriticalSectionRawMutex, BMS>, 
    ltc: &'static Mutex<CriticalSectionRawMutex, LTC6811>,
) {
    loop {
        let mut ltc_data = ltc.lock().await;

        match ltc_data.update().await {
            Ok(_) => {
                let bms_data = bms.lock().await;
                info!(
                    "Battery Status: Total: {}mV, Min: {}mV, Max: {}mV, Avg: {}mV",
                    bms_data.tot_volt(),
                    bms_data.min_volt(),
                    bms_data.max_volt(),
                    bms_data.avg_volt()
                );
                drop(bms_data);
            },
            Err(_) => defmt::error!("Failed to update battery data"),
        }
        embassy_time::Timer::after_millis(100).await;
    }
}

/// Sends USB packets using the serial interface
#[embassy_executor::task]
async fn send_usb(usb: &'static Mutex<CriticalSectionRawMutex, UsbDevice>) {
    loop {
        let mut usb_data = usb.lock().await;
        let _ = usb_data.write_usb().await;
        drop(usb_data);
    }
}