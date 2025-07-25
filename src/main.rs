#![no_std]
#![no_main]

use libm::roundf;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::adc::{Adc, Resolution};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use static_cell::StaticCell;
use embassy_stm32::peripherals::ADC1;

use crate::usb_serial::usb::Serial;
use crate::{can_management::{CanError, CanFrame}, ltc_management::ltc6811::MODE, types::_TEMPERATURES};

use defmt::info;
use panic_probe as _;

mod types;
mod can_management;
mod ltc_management;
mod usb_serial;

use types::{CanMsg, VOLTAGES, SLAVEBMS};
use can_management::{can_operation, CanController};
use ltc_management::{SpiDevice, LTC6811};
use usb_serial::prepare_config;

static BMS: StaticCell<Mutex<NoopRawMutex, SLAVEBMS>> = StaticCell::new();
static ERR_CHECK: StaticCell<Mutex<NoopRawMutex, Output>> = StaticCell::new();
static CAN: StaticCell<Mutex<NoopRawMutex, CanController>> = StaticCell::new();
static SPI: StaticCell<Mutex<NoopRawMutex, SpiDevice>> = StaticCell::new();
static LTC: StaticCell<Mutex<NoopRawMutex, LTC6811>> = StaticCell::new();
static IS_BALANCE: StaticCell<Mutex<NoopRawMutex, bool>> = StaticCell::new();

const VOLTAGE_OFFSET: f32 = 1650f32; //mV

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let p = embassy_stm32::init(prepare_config());

    let current_adc: embassy_stm32::adc::Adc<'static, ADC1, > = Adc::new(p.ADC1);
    let current_pin: embassy_stm32::peripherals::PA1 = p.PA1;

    let (can, rx1, tx1) = CanController::new_can2(p.CAN2, p.PB12, p.PB13, 500_000, p.CAN1, p.PA11, p.PA12).await;
    let can_mutex = Mutex::new(can);
    let can = StaticCell::init(&CAN, can_mutex);
    
    Serial::init(p.USB_OTG_FS, tx1, rx1, & spawner);

    let debug_led = Output::new(p.PC13, Level::Low, Speed::High);
    let temp_led = Output::new(p.PC9, Level::Low, Speed::High);
    let voltage_led = Output::new(p.PC11, Level::Low, Speed::High);

    let err_check = Output::new(p.PA2, Level::Low, Speed::High);
    let err_check_mutex = Mutex::new(err_check);
    let err_check = StaticCell::init(&ERR_CHECK, err_check_mutex);

    let is_balance = false;
    let is_balance_mutex = Mutex::new(is_balance);
    let is_balance = StaticCell::init(&IS_BALANCE, is_balance_mutex);

    let bms = setup_bms();
    let bms_mutex = Mutex::new(bms);
    let bms = StaticCell::init(&BMS, bms_mutex);

    spawner.spawn(current_sense(current_adc, current_pin, bms)).unwrap();
    
    spawner.spawn(send_can(bms, can)).unwrap();

    defmt::info!("Hello world over USB-CDC!");

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
    spawner.spawn(ltc_function(bms, ltc, err_check, can, debug_led, voltage_led, temp_led, is_balance)).unwrap();

    spawner.spawn(read_can(is_balance, can)).unwrap();

    loop {
        embassy_time::Timer::after_millis(10000).await;
        defmt::info!("FINO A QUI");
        defmt::panic!("CIAO");
    }
}

fn setup_bms() -> SLAVEBMS{
    let bms = SLAVEBMS::new();
    bms
}

#[embassy_executor::task]
async fn current_sense(
    mut adc: embassy_stm32::adc::Adc<'static, ADC1, >,
    mut curr_pin: embassy_stm32::peripherals::PA1,
    bms: &'static Mutex<NoopRawMutex, SLAVEBMS>
) {
    adc.set_resolution(Resolution::BITS12);
    embassy_time::Timer::after_millis(100).await;

    let mut count: u64 = 0;
    for _ in 0..10 {
        count = count.wrapping_add(adc.blocking_read(&mut curr_pin) as u64);
        embassy_time::Timer::after_millis(1).await;
    }

    let no_current_offset = ((count as f32)/10.0f32) * 3300f32 / (4095 as f32);
    let factor = no_current_offset / VOLTAGE_OFFSET;

    loop {
        count = 0;
        for _ in 0..50 {
            count = count.wrapping_add(adc.blocking_read(&mut curr_pin) as u64);
            embassy_time::Timer::after_micros(200).await;
        }

        let mut f_curr = ((count as f32)/50.0f32) * 3300f32 / (4095 as f32);
        f_curr = ((f_curr - no_current_offset)/(9.2f32*factor))*10000f32;

        let rounded: i32 = if f_curr >= 0.0f32 {
            roundf(f_curr).max(0.0) as i32
        } else {
            roundf(f_curr).min(0.0) as i32
        };

        let mut bms_data = bms.lock().await;

        bms_data.update_current(rounded);

        drop(bms_data);
    }
}


#[embassy_executor::task]
async fn send_can(
    bms: &'static Mutex<NoopRawMutex, SLAVEBMS>, 
    can: &'static Mutex<NoopRawMutex, CanController<'static>>,
){
    // let mut err_count: u16 = 0;
    loop {
        let bms_data = bms.lock().await;
        let mut can_data = can.lock().await;
        match can_operation(&bms_data, &mut can_data).await {
            Ok(_) => {},
            Err(_) => {}
        }
        drop(can_data);
        drop(bms_data);
        embassy_time::Timer::after_millis(100).await;
    }
}

#[embassy_executor::task]
async fn read_can(
    is_balance: &'static Mutex<NoopRawMutex, bool>,
    can: &'static Mutex<NoopRawMutex, CanController<'static>>,
){
    loop {
        let mut can_data = can.lock().await;
        match can_data.read().await {
            Ok(frame) => {
                let id = frame.id();
                let bytes = frame.bytes();
                drop(can_data);
                if id == CanMsg::Balancing.as_raw() {
                    if bytes[0] >= 0x1 as u8 {
                        let mut is_balance_data = is_balance.lock().await;
                        *is_balance_data = true;
                        drop(is_balance_data);

                    } else if bytes[0] == 0x0 as u8 {
                        let mut is_balance_data = is_balance.lock().await;
                        *is_balance_data = false;
                        drop(is_balance_data);
                    }
                }
            }
            Err(_) => {
                drop(can_data);
                info!("No messages");
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
    can: &'static Mutex<NoopRawMutex, CanController<'static>>,
    mut debug_led: Output<'static>,
    mut voltage_led: Output<'static>,
    mut temp_led: Output<'static>,
    is_balance: &'static Mutex<NoopRawMutex, bool>
) {
    let mut err_check_close = false;
    let mut time_now = embassy_time::Instant::now().as_millis();
    let mut first_close = false;

    loop {
        let mut ltc_data = ltc.lock().await;

        match ltc_data.update().await {
            Ok(_) => {
                defmt::info!("Battery Reading okay");
            },
            Err(_) => {
                defmt::error!("Failed to update battery data");
            }
        }
        
        let is_balance_data = is_balance.lock().await;
        let balance: bool = *is_balance_data;
        drop(is_balance_data);
        if balance == true{
            for _ in 0..5 {
                match ltc_data.update().await {
                Ok(_) => {
                    defmt::info!("Battery Reading okay");
                },
                Err(_) => {
                    defmt::error!("Failed to update battery data");
                }
            }
        }
        }

        drop(ltc_data);

        let bms_data = bms.lock().await;
        
        if &bms_data.min_volt() < &VOLTAGES::MINVOLTAGE.as_raw() || &bms_data.max_volt() > &VOLTAGES::MAXVOLTAGE.as_raw(){
            if embassy_time::Instant::now().as_millis() - time_now > 450 {
                err_check_close = false;
            }
        } else {
            err_check_close = true;
            first_close = true;
            time_now = embassy_time::Instant::now().as_millis();
        }

        if &bms_data.min_temp() < &_TEMPERATURES::_MINTEMP._as_raw() || &bms_data.max_temp() > &_TEMPERATURES::_MAXTEMP._as_raw() {
            if embassy_time::Instant::now().as_millis() > 2000 { 
                temp_led.set_high();
            }
        } else {
            temp_led.set_low();
        }
        drop(bms_data);

        let mut err_check_data = err_check.lock().await;
        if err_check_close {
            if embassy_time::Instant::now().as_millis() > 1000 {
                err_check_data.set_high();
            }
            debug_led.set_low();
        } else {
            err_check_data.set_low();
            if embassy_time::Instant::now().as_millis() > 2000 || first_close{
                voltage_led.set_high();
                debug_led.toggle();
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
                drop(can_data);
            }
        }
        drop(err_check_data);
        
        let mut is_balance_data = is_balance.lock().await;
        let balance: bool = *is_balance_data;
        if balance == true{
            let mut ltc_data = ltc.lock().await;
            if !ltc_data.check_need_balance().await {
                *is_balance_data = false;
            }
            let time = embassy_time::Instant::now().as_millis();
            while embassy_time::Instant::now().as_millis() - time < 10000 {
                ltc_data.set_mode(MODE::BALANCING).await;
                embassy_time::Timer::after_millis(5).await;
            }
            drop(ltc_data);
        } else {
            embassy_time::Timer::after_millis(1).await;
        }

        drop(is_balance_data);
    }
} 


