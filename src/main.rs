#![no_std]
#![no_main]

use core::str::from_utf8;

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Input, Level, Output, Pull, Speed};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

mod types;
mod can_management;
mod ltc_management;

use types::{CanMsg, Voltages, BMS};
use can_management::{CanController, can_operation};
use ltc_management::SpiDevice;

static BMS: StaticCell<Mutex<CriticalSectionRawMutex, BMS>> = StaticCell::new();
static SDC: StaticCell<Mutex<CriticalSectionRawMutex, Output>> = StaticCell::new();
static CAN: StaticCell<Mutex<CriticalSectionRawMutex, CanController>> = StaticCell::new();
static SPI: StaticCell<Mutex<CriticalSectionRawMutex, SpiDevice>> = StaticCell::new();

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut p = embassy_stm32::init(Default::default());

    let sdc = Output::new(p.PA2, Level::Low, Speed::High);
    let sdc_mutex = Mutex::new(sdc);
    let sdc = StaticCell::init(&SDC, sdc_mutex);

    let rx_pin = Input::new(&mut p.PB12, Pull::Up);
    core::mem::forget(rx_pin);
    
    let bms = setup_bms();
    let bms_mutex = Mutex::new(bms);
    let bms = StaticCell::init(&BMS, bms_mutex);

    let spi = SpiDevice::new(p.SPI1, p.PA5, p.PA7, p.PA6, p.PA4, p.DMA2_CH3, p.DMA2_CH0).await;
    let spi_mutex = Mutex::new(spi);
    let spi = StaticCell::init(&SPI, spi_mutex);
    spawner.spawn(spi_function(bms, spi)).unwrap();
    
    spawner.spawn(check_sdc(bms, sdc)).unwrap();
    let can = CanController::new_can2(p.CAN2, p.PB12, p.PB13, 500_000, p.CAN1, p.PA11, p.PA12).await;
    let can_mutex = Mutex::new(can);
    let can = StaticCell::init(&CAN, can_mutex);
    spawner.spawn(send_can(bms, can)).unwrap();
    spawner.spawn(read_can(can)).unwrap();


    loop {
        let mut bms_data = bms.lock().await;
        bms_data.update_cell(3, 21000);
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
    can: &'static Mutex<CriticalSectionRawMutex, CanController<'static>>
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
        embassy_time::Timer::after_millis(100).await;
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

        embassy_time::Timer::after_millis(100).await;
    }
}

#[embassy_executor::task]
async fn spi_function(
    bms: &'static Mutex<CriticalSectionRawMutex, BMS>, 
    spi: &'static Mutex<CriticalSectionRawMutex, SpiDevice<'static>>
) {
    let mut i: usize = 0;
    loop {
        let mut spi_data = spi.lock().await;
        spi_data.write(&[3]).await;
        let mut buf: [u8; 128] = [0; 128];
        spi_data.read(&mut buf).await;

        let txt = match from_utf8(&buf) {
            Ok(_) => {
                Ok(())
            }

            Err(_) => {
                Err("No Message")
            }
        };

        info!("read via spi+dma: {}", txt);
        let mut bms_data = bms.lock().await;
        bms_data.update_cell(10, 2999);
        i = i.wrapping_add(1);
        if i >= 12 {i = 0;}
        embassy_time::Timer::after_millis(100).await;

    }
}  

