#![no_std]
#![no_main]
#![allow(clippy::too_many_arguments, clippy::upper_case_acronyms)]

use core::sync::atomic::{AtomicBool, Ordering};
use embassy_executor::Spawner;
use embassy_stm32::adc::{Adc, AdcConfig, Averaging, Clock, Resolution};
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::pac::adc::vals::SampleTime as PacSampleTime;
use embassy_stm32::pac::adccommon::vals::Presc;
use embassy_stm32::peripherals::{ADC1, PA1};
use embassy_stm32::{bind_interrupts, dma, Peri};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use libm::roundf;
use static_cell::StaticCell;

use crate::can_management::CanError;
use crate::{can_management::CanFrame, ltc_management::ltc6811::MODE};

mod can_management;
mod hardware_config;
mod ltc_management;
mod types;

use can_management::{can_operation, can_operation_tech, CanController, CanReceiver};
use hardware_config::prepare_config;
use ltc_management::{SpiDevice, LTC6811};
use types::{CanMsg, SLAVEBMS, TEMPERATURES, VOLTAGES};

static BMS: StaticCell<Mutex<CriticalSectionRawMutex, SLAVEBMS>> = StaticCell::new();
static ERR_CHECK: StaticCell<Mutex<CriticalSectionRawMutex, Output>> = StaticCell::new();
static CAN: StaticCell<Mutex<CriticalSectionRawMutex, CanController>> = StaticCell::new();
static SPI: StaticCell<Mutex<CriticalSectionRawMutex, SpiDevice>> = StaticCell::new();
static LTC: StaticCell<Mutex<CriticalSectionRawMutex, LTC6811>> = StaticCell::new();
static IS_BALANCE: StaticCell<Mutex<CriticalSectionRawMutex, bool>> = StaticCell::new();
static IS_TECH: StaticCell<Mutex<CriticalSectionRawMutex, bool>> = StaticCell::new();
static FAULT_TEMP: AtomicBool = AtomicBool::new(true);
static CURRENT_CALIBRATED: AtomicBool = AtomicBool::new(false);

const ACS773_100B_COUNTS_PER_AMP: f32 = 4095.0 * 13.2 / 3300.0;
const ENABLE_DELAY_MS: u64 = 1_000;

use panic_probe as _;

bind_interrupts!(struct SpiIrqs {
    GPDMA1_CHANNEL0 => dma::InterruptHandler<embassy_stm32::peripherals::GPDMA1_CH0>;
    GPDMA1_CHANNEL1 => dma::InterruptHandler<embassy_stm32::peripherals::GPDMA1_CH1>;
});

fn enable_early_debug_led() {
    use embassy_stm32::pac::gpio::vals::{Moder, Ot, Pupdr};

    const PIN: usize = 14;
    embassy_stm32::pac::RCC
        .ahb2enr()
        .modify(|w| w.set_gpioben(true));
    embassy_stm32::pac::GPIOB
        .pupdr()
        .modify(|w| w.set_pupdr(PIN, Pupdr::Floating));
    embassy_stm32::pac::GPIOB
        .otyper()
        .modify(|w| w.set_ot(PIN, Ot::PushPull));
    embassy_stm32::pac::GPIOB
        .bsrr()
        .write(|w| w.set_bs(PIN, true));
    embassy_stm32::pac::GPIOB
        .moder()
        .modify(|w| w.set_moder(PIN, Moder::Output));
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    enable_early_debug_led();
    let p = embassy_stm32::init(prepare_config());
    let adc_config = AdcConfig {
        clock: Some(Clock::Async { div: Presc::Div4 }),
        resolution: Some(Resolution::Bits12),
        averaging: Some(Averaging::Disabled),
    };
    let current_adc: Adc<'static, ADC1> = Adc::new_with_config(p.ADC1, adc_config);
    let current_pin = p.PA1;

    let (can, can_rx) = CanController::new_fdcan1(p.FDCAN1, p.PD0, p.PD1, 1_000_000);
    let can = StaticCell::init(&CAN, Mutex::new(can));
    let bms = StaticCell::init(&BMS, Mutex::new(SLAVEBMS::new()));
    let is_balance = StaticCell::init(&IS_BALANCE, Mutex::new(false));
    let is_tech = StaticCell::init(&IS_TECH, Mutex::new(true));

    let debug_led = Output::new(p.PB14, Level::High, Speed::Low);
    let temp_led = Output::new(p.PB12, Level::High, Speed::Low);
    let voltage_led = Output::new(p.PB13, Level::High, Speed::Low);
    let err_check = StaticCell::init(
        &ERR_CHECK,
        Mutex::new(Output::new(p.PB15, Level::Low, Speed::Low)),
    );

    spawner.spawn(current_sense(current_adc, current_pin, bms).unwrap());
    spawner.spawn(send_can(bms, can, is_tech).unwrap());
    spawner.spawn(read_can(is_balance, can_rx, is_tech).unwrap());

    let spi = SpiDevice::new(
        p.SPI1,
        p.PA5,
        p.PA7,
        p.PA6,
        p.PA4,
        p.GPDMA1_CH0,
        p.GPDMA1_CH1,
        SpiIrqs,
    )
    .await;
    let spi = StaticCell::init(&SPI, Mutex::new(spi));
    let ltc = StaticCell::init(&LTC, Mutex::new(LTC6811::new(spi, bms).await));

    spawner.spawn(
        ltc_function(
            bms,
            ltc,
            err_check,
            can,
            debug_led,
            voltage_led,
            temp_led,
            is_balance,
        )
        .unwrap(),
    );

    core::future::pending().await
}

#[embassy_executor::task]
async fn current_sense(
    mut adc: embassy_stm32::adc::Adc<'static, ADC1>,
    mut curr_pin: Peri<'static, PA1>,
    bms: &'static Mutex<CriticalSectionRawMutex, SLAVEBMS>,
) {
    embassy_time::Timer::after_secs(2).await;

    let mut count: u64 = 0;
    for _ in 0..512 {
        count = count.wrapping_add(adc1_blocking_read(&mut adc, &mut curr_pin, 1) as u64);
        embassy_time::Timer::after_millis(1).await;
    }

    let no_current_offset_counts = count as f32 / 512.0;
    CURRENT_CALIBRATED.store(true, Ordering::Release);

    loop {
        count = 0;
        for _ in 0..50 {
            count = count.wrapping_add(adc1_blocking_read(&mut adc, &mut curr_pin, 1) as u64);
            embassy_time::Timer::after_micros(200).await;
        }

        let average_counts = count as f32 / 50.0;
        let f_curr =
            ((average_counts - no_current_offset_counts) / ACS773_100B_COUNTS_PER_AMP) * 10000.0;

        let rounded: i32 = if f_curr >= 0.0f32 {
            roundf(f_curr).max(0.0) as i32
        } else {
            roundf(f_curr).min(0.0) as i32
        };

        let mut bms_data = bms.lock().await;
        bms_data.update_current(rounded);
        drop(bms_data);

        embassy_time::Timer::after_millis(10).await;
    }
}

fn adc1_blocking_read(
    _adc: &mut Adc<'static, ADC1>,
    _pin: &mut Peri<'static, PA1>,
    channel: u8,
) -> u16 {
    let regs = embassy_stm32::pac::ADC1;

    embassy_stm32::pac::GPIOA
        .moder()
        .modify(|w| w.set_moder(1, embassy_stm32::pac::gpio::vals::Moder::Analog));

    if regs.cr().read().adstart() {
        regs.cr().modify(|w| w.set_adstp(true));
        while regs.cr().read().adstart() {}
    }
    if regs.cr().read().aden() {
        regs.cr().modify(|w| w.set_addis(true));
        while regs.cr().read().aden() {}
    }

    regs.difsel().modify(|w| {
        let differential_channels = w.difsel() & !(1u32 << channel);
        w.set_difsel(differential_channels);
    });
    regs.sqr1().write(|w| {
        w.set_l(0);
        w.set_sq(0, channel);
    });
    if channel < 10 {
        regs.smpr1()
            .modify(|w| w.set_smp(channel as usize, PacSampleTime::Cycles925));
    } else {
        regs.smpr2()
            .modify(|w| w.set_smp((channel - 10) as usize, PacSampleTime::Cycles925));
    }

    regs.isr().write(|w| w.set_adrdy(true));
    regs.cr().modify(|w| w.set_aden(true));
    while !regs.isr().read().adrdy() {}

    regs.isr().write(|w| {
        w.set_eoc(true);
        w.set_eos(true);
        w.set_ovr(true);
    });
    regs.cr().modify(|w| w.set_adstart(true));
    while !regs.isr().read().eoc() {}

    regs.dr().read().rdata()
}

#[embassy_executor::task]
async fn send_can(
    bms: &'static Mutex<CriticalSectionRawMutex, SLAVEBMS>,
    can: &'static Mutex<CriticalSectionRawMutex, CanController<'static>>,
    is_tech: &'static Mutex<CriticalSectionRawMutex, bool>,
) {
    loop {
        let bms_data = bms.lock().await;
        let mut can_data = can.lock().await;
        let _ = can_operation(&bms_data, &mut can_data, FAULT_TEMP.load(Ordering::Relaxed)).await;
        drop(can_data);
        drop(bms_data);
        embassy_time::Timer::after_millis(10).await;

        let is_tech_data = is_tech.lock().await;
        let tech: bool = *is_tech_data;
        drop(is_tech_data);
        embassy_time::Timer::after_millis(1).await;
        if tech {
            let bms_data = bms.lock().await;
            let mut can_data = can.lock().await;
            let _ = can_operation_tech(&bms_data, &mut can_data).await;
            drop(can_data);
            drop(bms_data);
        }
        embassy_time::Timer::after_millis(189).await;
    }
}

#[embassy_executor::task]
async fn read_can(
    is_balance: &'static Mutex<CriticalSectionRawMutex, bool>,
    mut can: CanReceiver<'static>,
    is_tech: &'static Mutex<CriticalSectionRawMutex, bool>,
) {
    loop {
        match can.read().await {
            Ok(frame) => {
                let id = frame.id();
                let bytes = frame.bytes();
                if id == CanMsg::Balancing.as_raw() {
                    let mut is_balance_data = is_balance.lock().await;
                    *is_balance_data = bytes.first().copied().unwrap_or(0) >= 0x1;
                    drop(is_balance_data);
                }
                if id == CanMsg::Tech.as_raw() {
                    let mut is_tech_data = is_tech.lock().await;
                    *is_tech_data = bytes.first().copied().unwrap_or(0) >= 0x1;
                    drop(is_tech_data);
                }
            }
            Err(_) => {
                embassy_time::Timer::after_millis(1).await;
            }
        }
    }
}

#[embassy_executor::task]
#[allow(clippy::too_many_arguments)]
async fn ltc_function(
    bms: &'static Mutex<CriticalSectionRawMutex, SLAVEBMS>,
    ltc: &'static Mutex<CriticalSectionRawMutex, LTC6811>,
    err_check: &'static Mutex<CriticalSectionRawMutex, Output<'static>>,
    can: &'static Mutex<CriticalSectionRawMutex, CanController<'static>>,
    mut debug_led: Output<'static>,
    mut voltage_led: Output<'static>,
    mut temp_led: Output<'static>,
    is_balance: &'static Mutex<CriticalSectionRawMutex, bool>,
) {
    while !CURRENT_CALIBRATED.load(Ordering::Acquire) {
        embassy_time::Timer::after_millis(1).await;
    }

    let _ = {
        let mut ltc_data = ltc.lock().await;
        matches!(
            embassy_time::with_timeout(embassy_time::Duration::from_millis(500), ltc_data.init())
                .await,
            Ok(Ok(()))
        )
    };

    let mut time_err_volt = embassy_time::Instant::now().as_millis();
    let mut time_err_temp = embassy_time::Instant::now().as_millis();
    let mut fault_temp: bool = false;
    let mut fault_volt: bool = false;
    let mut no_error_since: Option<u64> = None;

    loop {
        let mut ltc_data = ltc.lock().await;

        let _ = matches!(
            embassy_time::with_timeout(embassy_time::Duration::from_millis(500), ltc_data.update())
                .await,
            Ok(Ok(()))
        );

        let is_balance_data = is_balance.lock().await;
        let balance: bool = *is_balance_data;
        drop(is_balance_data);
        if balance {
            for _ in 0..5 {
                let _ = ltc_data.update().await;
            }
        }

        drop(ltc_data);

        let bms_data = bms.lock().await;
        let voltage_valid = bms_data.has_all_cell_samples()
            && bms_data.min_volt() >= VOLTAGES::MINVOLTAGE.as_raw()
            && bms_data.max_volt() <= VOLTAGES::MAXVOLTAGE.as_raw();
        if !voltage_valid {
            if embassy_time::Instant::now().as_millis() - time_err_volt > 500 {
                voltage_led.set_high();
                fault_volt = true;
            }
        } else {
            fault_volt = false;
            time_err_volt = embassy_time::Instant::now().as_millis();
            voltage_led.set_low();
        }

        let temperature_valid = bms_data.has_all_temperature_samples()
            && bms_data.min_temp() >= TEMPERATURES::MINTEMP._as_raw()
            && bms_data.max_temp() <= TEMPERATURES::MAXTEMP._as_raw();
        if !temperature_valid {
            if embassy_time::Instant::now().as_millis() - time_err_temp > 500 {
                temp_led.set_high();
                fault_temp = true;
            }
        } else {
            fault_temp = false;
            time_err_temp = embassy_time::Instant::now().as_millis();
            temp_led.set_low();
        }
        FAULT_TEMP.store(
            fault_temp || !bms_data.has_all_temperature_samples(),
            Ordering::Relaxed,
        );

        drop(bms_data);

        let mut err_check_data = err_check.lock().await;
        if !(fault_temp || fault_volt) {
            let now = embassy_time::Instant::now().as_millis();
            if CURRENT_CALIBRATED.load(Ordering::Acquire) && voltage_valid && temperature_valid {
                let valid_since = *no_error_since.get_or_insert(now);
                if now - valid_since >= ENABLE_DELAY_MS {
                    err_check_data.set_high();
                } else {
                    err_check_data.set_low();
                }
            } else {
                no_error_since = None;
                err_check_data.set_low();
            }
            debug_led.set_high();
        } else {
            no_error_since = None;
            err_check_data.set_low();
            if embassy_time::Instant::now().as_millis() > 2000 {
                debug_led.toggle();
                let mut can_data = can.lock().await;
                let can_second = [u8::from(fault_temp || fault_volt)];
                let frame_send = CanFrame::new(CanMsg::ErrorId.as_raw(), &can_second);
                match can_data.write(&frame_send).await {
                    Ok(_) => {}
                    Err(CanError::Timeout) => {}
                    Err(_) => {}
                }
                drop(can_data);
                embassy_time::Timer::after_millis(200).await;
            }
        }
        drop(err_check_data);

        let mut is_balance_data = is_balance.lock().await;
        let balance: bool = *is_balance_data;
        if balance {
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
            embassy_time::Timer::after_millis(5).await;
        }

        drop(is_balance_data);
        embassy_time::Timer::after_millis(5).await;
    }
}
