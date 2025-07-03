#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use static_cell::StaticCell;
use crate::can_management::{CanError, CanFrame};

use {defmt_rtt as _, panic_probe as _};

mod types;
mod can_management;
mod ltc_management;

use types::{CanMsg, VOLTAGES, SLAVEBMS};
use can_management::{can_operation, CanController};
use ltc_management::{ltc6811::MODE, SpiDevice, LTC6811};

static BMS: StaticCell<Mutex<NoopRawMutex, SLAVEBMS>> = StaticCell::new();
static ERR_CHECK: StaticCell<Mutex<NoopRawMutex, Output>> = StaticCell::new();
static CAN: StaticCell<Mutex<NoopRawMutex, CanController>> = StaticCell::new();
static SPI: StaticCell<Mutex<NoopRawMutex, SpiDevice>> = StaticCell::new();
static LTC: StaticCell<Mutex<NoopRawMutex, LTC6811>> = StaticCell::new();

#[embassy_executor::main]
async fn main(spawner: Spawner) -> !{
    let p = embassy_stm32::init(Default::default());

    let err_check = Output::new(p.PA2, Level::Low, Speed::High);
    let err_check_mutex = Mutex::new(err_check);
    let err_check = StaticCell::init(&ERR_CHECK, err_check_mutex);

    let bms = setup_bms();
    let bms_mutex = Mutex::new(bms);
    let bms = StaticCell::init(&BMS, bms_mutex);

    let can = CanController::new_can2(p.CAN2, p.PB12, p.PB13, 500_000, p.CAN1, p.PA11, p.PA12).await;
    let can_mutex = Mutex::new(can);
    let can = StaticCell::init(&CAN, can_mutex);
    spawner.spawn(send_can(bms, can)).unwrap();

    let spi: SpiDevice<'static> = SpiDevice::new(p.SPI1, p.PA5, p.PA7, p.PA6, p.PA4, p.DMA2_CH3, p.DMA2_CH0).await;
    let spi_mutex = Mutex::new(spi);
    let spi = StaticCell::init(&SPI, spi_mutex);

    let mut ltc = LTC6811::new(spi, bms).await;  // Initialize LTC6811
    match ltc.init().await {
        Ok(_) => defmt::info!("LTC6811 initialized successfully"),
        Err(_) => defmt::error!("Failed to initialize LTC6811"),
    }

    let ltc_mutex = Mutex::new(ltc);
    let ltc = StaticCell::init(&LTC, ltc_mutex);
    spawner.spawn(ltc_function(bms, ltc, err_check, can)).unwrap();

    spawner.spawn(read_can(ltc, can)).unwrap();

    loop {        
        embassy_time::Timer::after_millis(10).await;
    }
}

fn setup_bms() -> SLAVEBMS{
    let bms = SLAVEBMS::new();
    bms
}

#[embassy_executor::task]
async fn send_can(
    bms: &'static Mutex<NoopRawMutex, SLAVEBMS>, 
    can: &'static Mutex<NoopRawMutex, CanController<'static>>,
){
    loop {
        let bms_data = bms.lock().await;
        let mut can_data = can.lock().await;
        can_operation(&bms_data, &mut can_data).await;
        drop(can_data);
        drop(bms_data);
        embassy_time::Timer::after_millis(100).await;
    }
}

#[embassy_executor::task]
async fn read_can(
    ltc: &'static Mutex<NoopRawMutex, LTC6811>,
    can: &'static Mutex<NoopRawMutex, CanController<'static>>
){
    let mut time_now = embassy_time::Instant::now().as_millis();
    loop {
        let mut can_data = can.lock().await;
        match can_data.read().await {
            Ok(frame) => {
                let id = frame.id();
                let bytes = frame.bytes();
                drop(can_data);
                if id == CanMsg::Balancing.as_raw() {
                    if bytes[0] == 0x1 as u8 {
                        let mut ltc_data = ltc.lock().await;
                        ltc_data.set_mode(MODE::BALANCING).await;
                        drop(ltc_data);

                    } else {
                        let mut ltc_data = ltc.lock().await;
                        ltc_data.set_mode(MODE::NORMAL).await;
                        drop(ltc_data);
                    }
                }
                time_now = embassy_time::Instant::now().as_millis();
            }
            Err(_) => {
                drop(can_data);
                info!("No messages");
                if (embassy_time::Instant::now().as_millis() - time_now) > 10000 {
                    // let mut ltc_data = ltc.lock().await;
                    // ltc_data.set_mode(MODE::SLEEP);
                    // drop(ltc_data);
                }
            }
        }
        embassy_time::Timer::after_micros(10).await;
    }
}

#[embassy_executor::task]
async fn ltc_function(
    bms: &'static Mutex<NoopRawMutex, SLAVEBMS>, 
    ltc: &'static Mutex<NoopRawMutex, LTC6811>,
    err_check: &'static Mutex<NoopRawMutex, Output<'static>>,
    can: &'static Mutex<NoopRawMutex, CanController<'static>>
) {
    let mut err_check_close = true;
    let mut time_now = embassy_time::Instant::now().as_millis();

    loop {
        let mut ltc_data = ltc.lock().await;
        
        if ltc_data.get_mode() == MODE::BALANCING {
            let _ = ltc_data.balance_cells().await;
            embassy_time::Timer::after_millis(2000).await;
        } else {
            embassy_time::Timer::after_millis(5).await;
        }

        match ltc_data.update().await {
            Ok(_) => {
                // Access BMS data
                let bms_data = bms.lock().await;
                
                // Log battery information
                defmt::info!(
                    "Battery Status: Total: {}mV, Min: {}mV, Max: {}mV, Avg: {}mV",
                    bms_data.tot_volt(),
                    bms_data.min_volt(),
                    bms_data.max_volt(),
                    bms_data.avg_volt()
                );
                
                drop(bms_data);
            },
            Err(_) => {
                defmt::error!("Failed to update battery data");
            }
        }
        
        let bms_data = bms.lock().await;
        
        if &bms_data.min_volt() < &VOLTAGES::MINVOLTAGE.as_raw() || &bms_data.max_volt() > &VOLTAGES::MAXVOLTAGE.as_raw(){
            if embassy_time::Instant::now().as_millis() - time_now > 450 {
                err_check_close = false;
            }
        } else {
            err_check_close = true;
            time_now = embassy_time::Instant::now().as_millis();
        }
        drop(bms_data);

        let mut err_check_data = err_check.lock().await;
        if err_check_close {
            err_check_data.set_high();
        } else {
            err_check_data.set_low();
            let mut can_data = can.lock().await;
            let can_second = [
                1
            ];

            let frame_send = CanFrame::new(CanMsg::ErrorId.as_raw(), &can_second);
            match can_data.write(&frame_send).await {
                Ok(_) => {
                    info!("Message sent! {}", &frame_send.id());
                    for i in 0..frame_send.len() {
                        info!("Byte: {}: {}", i, &frame_send.byte(i));
                    }
                }

                Err(CanError::Timeout) => {
                    info!("Timeout Can connection");
                }

                Err(_) => {
                    info!("Can write error");
                }
            }
        }
        drop(err_check_data);
    }
}  

