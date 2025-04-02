#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

mod types;
mod can_management;
mod ltc_management;

use types::{CanMsg, Voltages, BMS};
use can_management::{can_operation, CanController};
use ltc_management::{SpiDevice, LTC6811};

static BMS: StaticCell<Mutex<CriticalSectionRawMutex, BMS>> = StaticCell::new();
static SDC: StaticCell<Mutex<CriticalSectionRawMutex, Output>> = StaticCell::new();
static CAN: StaticCell<Mutex<CriticalSectionRawMutex, CanController>> = StaticCell::new();
static SPI: StaticCell<Mutex<CriticalSectionRawMutex, SpiDevice>> = StaticCell::new();
static LTC: StaticCell<Mutex<CriticalSectionRawMutex, LTC6811>> = StaticCell::new();

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());

    let sdc = Output::new(p.PA2, Level::Low, Speed::High);
    let sdc_mutex = Mutex::new(sdc);
    let sdc = StaticCell::init(&SDC, sdc_mutex);

    let bms = setup_bms();
    let bms_mutex = Mutex::new(bms);
    let bms = StaticCell::init(&BMS, bms_mutex);

    spawner.spawn(check_sdc(bms, sdc)).unwrap();
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
    spawner.spawn(ltc_function(bms, ltc)).unwrap();


    loop {
        let mut bms_data = bms.lock().await;
        // bms_data.update_cell(3, 21000);
        drop(bms_data);
        
        embassy_time::Timer::after_millis(10).await;
    }
}

fn setup_bms() -> BMS{
    let bms = BMS::new();
    bms
}

#[embassy_executor::task]
async fn send_can(
    bms: &'static Mutex<CriticalSectionRawMutex, BMS>, 
    can: &'static Mutex<CriticalSectionRawMutex, CanController<'static>>,
    // spi: &'static Mutex<CriticalSectionRawMutex, SpiDevice<'static>>
){
    loop {
        let bms_data = bms.lock().await;
        let mut can_data = can.lock().await;
        can_operation(&bms_data, &mut can_data).await;
        drop(can_data);
        drop(bms_data);
        embassy_time::Timer::after_millis(1000).await;
    }
}

#[embassy_executor::task]
async fn read_can(
    can: &'static Mutex<CriticalSectionRawMutex, CanController<'static>>
){
    loop {
        let mut can_data = can.lock().await;
        match can_data.read().await {
            Ok(frame) => info!("Out: {}", frame.byte(0)),
            Err(_) => info!("No messages"),
        }
        drop(can_data);
        embassy_time::Timer::after_millis(10).await;
    }
}

#[embassy_executor::task]
async fn check_sdc(
        bms: &'static Mutex<CriticalSectionRawMutex, BMS>, 
        sdc: &'static Mutex<CriticalSectionRawMutex, Output<'static>>
){
    loop {
        let bms_data = bms.lock().await;
        let mut sdc_close = true;
        
        for cell in &bms_data.cell_volts {
            if cell < &Voltages::MinVoltage.as_raw() || cell > &Voltages::MaxVoltage.as_raw(){
                sdc_close = false;
                break;
            }
        }
        drop(bms_data);

        let mut sdc_data = sdc.lock().await;
        if sdc_close {
            sdc_data.set_high();
        } else {
            sdc_data.set_low();
        }
        drop(sdc_data);

        embassy_time::Timer::after_millis(10).await;
    }
}

#[embassy_executor::task]
async fn ltc_function(
    bms: &'static Mutex<CriticalSectionRawMutex, BMS>, 
    ltc: &'static Mutex<CriticalSectionRawMutex, LTC6811>,
) {
    loop {
        let mut ltc_data = ltc.lock().await;

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
                
            },
            Err(_) => defmt::error!("Failed to update battery data"),
        }
    }
}  

