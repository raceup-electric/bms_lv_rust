#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Input, Level, Output, Pull, Speed};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};
use core::str::from_utf8;

mod types;
mod can_management;
mod ltc_management;

use types::{CanMsg, Voltages, BMS};
use can_management::{CanController, can_operation};
use ltc_management::SpiDevice;

static BMS: StaticCell<Mutex<CriticalSectionRawMutex, BMS>> = StaticCell::new();
static SDC: StaticCell<Mutex<CriticalSectionRawMutex, Output>> = StaticCell::new();

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut p = embassy_stm32::init(Default::default());

    let sdc = Output::new(p.PA2, Level::Low, Speed::High);
    let sdc_mutex = Mutex::new(sdc);
    let sdc = StaticCell::init(&SDC, sdc_mutex);

    let rx_pin = Input::new(&mut p.PA11, Pull::Up);
    core::mem::forget(rx_pin);
    
    let bms = setup_bms();
    let bms_mutex = Mutex::new(bms);
    let bms = StaticCell::init(&BMS, bms_mutex);
    
    spawner.spawn(check_sdc(bms, sdc)).unwrap();
    
    let mut spi = SpiDevice::new(p.SPI1, p.PA5, p.PA7, p.PA6, p.PA4, p.DMA2_CH3, p.DMA2_CH0).await;
    let mut can = CanController::new_can2(p.CAN2, p.PB12, p.PB13, 500_000).await;
    
    let mut i: u8 = 0;

    loop {
        let mut bms_data = bms.lock().await;
        bms_data.update_cell(3, 21000);
        can_operation(&bms_data, &mut can).await;
        drop(bms_data);
        
        match can.read().await {
            Ok(frame) => info!("Out: {}", frame.byte(0)),
            Err(_) => info!("No messages"),
        }
        i = i.wrapping_add(1);
        embassy_time::Timer::after_millis(10).await;

        spi.write(&[3]).await;
        let mut buf: [u8; 128] = [0; 128];
        spi.read(&mut buf).await;
        info!("read via spi+dma: {}", from_utf8(&buf).unwrap());
    }
}

fn setup_bms() -> BMS{
    let bms = BMS::new();
    bms
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


