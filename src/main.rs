#![no_std]
#![no_main]
#![allow(clippy::too_many_arguments, clippy::upper_case_acronyms)]

use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use embassy_executor::Spawner;
use embassy_stm32::adc::{Adc, AdcConfig, Averaging, Clock, Resolution, VrefInt};
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
#[cfg(feature = "ltc-hardware")]
use crate::usb_serial::Serial;
use crate::{can_management::CanFrame, ltc_management::ltc6811::MODE};

mod can_management;
mod ltc_management;
mod types;
mod usb_serial;

use can_management::{can_operation, can_operation_tech, CanController, CanReceiver};
use ltc_management::{SpiDevice, LTC6811};
use types::{CanMsg, SLAVEBMS, TEMPERATURES, VOLTAGES};
use usb_serial::prepare_config;

static BMS: StaticCell<Mutex<CriticalSectionRawMutex, SLAVEBMS>> = StaticCell::new();
static ERR_CHECK: StaticCell<Mutex<CriticalSectionRawMutex, Output>> = StaticCell::new();
static CAN: StaticCell<Mutex<CriticalSectionRawMutex, CanController>> = StaticCell::new();
static SPI: StaticCell<Mutex<CriticalSectionRawMutex, SpiDevice>> = StaticCell::new();
static LTC: StaticCell<Mutex<CriticalSectionRawMutex, LTC6811>> = StaticCell::new();
static IS_BALANCE: StaticCell<Mutex<CriticalSectionRawMutex, bool>> = StaticCell::new();
static IS_TECH: StaticCell<Mutex<CriticalSectionRawMutex, bool>> = StaticCell::new();
static FAULT_TEMP: AtomicBool = AtomicBool::new(true);
static CURRENT_CALIBRATED: AtomicBool = AtomicBool::new(false);
static CURRENT_OFFSET_Q8: AtomicI32 = AtomicI32::new(0);
static CURRENT_AVERAGE_Q8: AtomicI32 = AtomicI32::new(0);
static CURRENT_CENTIAMPS: AtomicI32 = AtomicI32::new(0);
static ADC_VREFINT_RAW: AtomicI32 = AtomicI32::new(0);
static ADC_VREF_MILLIVOLTS: AtomicI32 = AtomicI32::new(0);
static ADC_INPUT_MILLIVOLTS: AtomicI32 = AtomicI32::new(0);
static ADC_DR_SNAPSHOT: AtomicU32 = AtomicU32::new(0);
static ADC_ISR_SNAPSHOT: AtomicU32 = AtomicU32::new(0);
static ADC_CR_SNAPSHOT: AtomicU32 = AtomicU32::new(0);
static ADC_CFGR_SNAPSHOT: AtomicU32 = AtomicU32::new(0);
static ADC_SQR1_SNAPSHOT: AtomicU32 = AtomicU32::new(0);
static ADC_SMPR1_SNAPSHOT: AtomicU32 = AtomicU32::new(0);
static ADC_SMPR2_SNAPSHOT: AtomicU32 = AtomicU32::new(0);
static ADC_CCR_SNAPSHOT: AtomicU32 = AtomicU32::new(0);
static ADC_DIFSEL_SNAPSHOT: AtomicU32 = AtomicU32::new(0);
static ADC_CALFACT_SNAPSHOT: AtomicU32 = AtomicU32::new(0);
static ADC_GPIO_MODER_SNAPSHOT: AtomicU32 = AtomicU32::new(0);
static ADC_GPIO_PUPDR_SNAPSHOT: AtomicU32 = AtomicU32::new(0);
static ADC_RCC_CCIPR5_SNAPSHOT: AtomicU32 = AtomicU32::new(0);
static ADC_OR_SNAPSHOT: AtomicU32 = AtomicU32::new(0);
static ADC_CFGR2_SNAPSHOT: AtomicU32 = AtomicU32::new(0);

const ACS773_100B_COUNTS_PER_AMP: f32 = 4095.0 * 13.2 / 3300.0;

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

    #[cfg(feature = "ltc-hardware")]
    Serial::init(p.USB, p.PA12, p.PA11, &spawner);

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
    let mut vrefint: VrefInt = adc.enable_vrefint();
    embassy_time::Timer::after_secs(2).await;

    let mut count: u64 = 0;
    for _ in 0..512 {
        count = count.wrapping_add(adc1_blocking_read(&mut adc, &mut curr_pin, 1) as u64);
        embassy_time::Timer::after_millis(1).await;
    }

    let no_current_offset_counts = count as f32 / 512.0;
    CURRENT_OFFSET_Q8.store(
        roundf(no_current_offset_counts * 256.0) as i32,
        Ordering::Relaxed,
    );
    CURRENT_CALIBRATED.store(true, Ordering::Release);

    loop {
        count = 0;
        for _ in 0..50 {
            count = count.wrapping_add(adc1_blocking_read(&mut adc, &mut curr_pin, 1) as u64);
            embassy_time::Timer::after_micros(200).await;
        }

        let average_counts = count as f32 / 50.0;
        CURRENT_AVERAGE_Q8.store(roundf(average_counts * 256.0) as i32, Ordering::Relaxed);

        let _ = &mut vrefint;
        let _ = adc1_blocking_read(&mut adc, &mut curr_pin, 17);
        let vrefint_raw = adc1_blocking_read(&mut adc, &mut curr_pin, 17);
        ADC_VREFINT_RAW.store(vrefint_raw as i32, Ordering::Relaxed);
        if vrefint_raw != 0 {
            let vref_mv = (1_216u32 * 4_095) / u32::from(vrefint_raw);
            let input_mv = ((average_counts * vref_mv as f32) / 4_095.0) as i32;
            ADC_VREF_MILLIVOLTS.store(vref_mv as i32, Ordering::Relaxed);
            ADC_INPUT_MILLIVOLTS.store(input_mv, Ordering::Relaxed);
        }
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
        CURRENT_CENTIAMPS.store(rounded / 100, Ordering::Relaxed);

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

    let isr = regs.isr().read().0;
    let cr = regs.cr().read().0;
    let cfgr = regs.cfgr().read().0;
    let dr = regs.dr().read().0;
    ADC_DR_SNAPSHOT.store(dr, Ordering::Relaxed);
    ADC_ISR_SNAPSHOT.store(isr, Ordering::Relaxed);
    ADC_CR_SNAPSHOT.store(cr, Ordering::Relaxed);
    ADC_CFGR_SNAPSHOT.store(cfgr, Ordering::Relaxed);
    ADC_SQR1_SNAPSHOT.store(regs.sqr1().read().0, Ordering::Relaxed);
    ADC_SMPR1_SNAPSHOT.store(regs.smpr1().read().0, Ordering::Relaxed);
    ADC_SMPR2_SNAPSHOT.store(regs.smpr2().read().0, Ordering::Relaxed);
    ADC_CCR_SNAPSHOT.store(
        embassy_stm32::pac::ADC12_COMMON.ccr().read().0,
        Ordering::Relaxed,
    );
    ADC_DIFSEL_SNAPSHOT.store(regs.difsel().read().0, Ordering::Relaxed);
    ADC_CALFACT_SNAPSHOT.store(regs.calfact().read().0, Ordering::Relaxed);
    ADC_GPIO_MODER_SNAPSHOT.store(
        embassy_stm32::pac::GPIOA.moder().read().0,
        Ordering::Relaxed,
    );
    ADC_GPIO_PUPDR_SNAPSHOT.store(
        embassy_stm32::pac::GPIOA.pupdr().read().0,
        Ordering::Relaxed,
    );
    ADC_RCC_CCIPR5_SNAPSHOT.store(embassy_stm32::pac::RCC.ccipr5().read().0, Ordering::Relaxed);
    ADC_OR_SNAPSHOT.store(regs.or().read().0, Ordering::Relaxed);
    ADC_CFGR2_SNAPSHOT.store(regs.cfgr2().read().0, Ordering::Relaxed);
    dr as u16
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
    let mut time_send_log = embassy_time::Instant::now().as_millis();

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

        let ltc_diagnostics = ltc_data.diagnostics();

        drop(ltc_data);

        let bms_data = bms.lock().await;
        if !bms_data.has_all_cell_samples()
            || bms_data.min_volt() < VOLTAGES::MINVOLTAGE.as_raw()
            || bms_data.max_volt() > VOLTAGES::MAXVOLTAGE.as_raw()
        {
            if embassy_time::Instant::now().as_millis() - time_err_volt > 850 {
                voltage_led.set_high();
                fault_volt = true;
            }
        } else {
            fault_volt = false;
            time_err_volt = embassy_time::Instant::now().as_millis();
            voltage_led.set_low();
        }

        if !bms_data.has_all_temperature_samples()
            || bms_data.min_temp() < TEMPERATURES::MINTEMP._as_raw()
            || bms_data.max_temp() > TEMPERATURES::MAXTEMP._as_raw()
        {
            if embassy_time::Instant::now().as_millis() - time_err_temp > 450 {
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

        if embassy_time::Instant::now().as_millis() - time_send_log > 1000 {
            let cell_mask = bms_data.seen_cell_sample_mask();
            let temperature_mask = bms_data.seen_temperature_sample_mask();
            let measurement_count = bms_data.measurement_count();
            let diagnostic = [
                cell_mask as u8,
                (cell_mask >> 8) as u8,
                temperature_mask as u8,
                (temperature_mask >> 8) as u8,
                measurement_count as u8,
                (measurement_count >> 8) as u8,
                (measurement_count >> 16) as u8,
                (measurement_count >> 24) as u8,
            ];
            let mut can_data = can.lock().await;
            let _ = can_data
                .write(&CanFrame::new(CanMsg::DiagnosticId.as_raw(), &diagnostic))
                .await;

            let offset_q8 = CURRENT_OFFSET_Q8.load(Ordering::Relaxed);
            let average_q8 = CURRENT_AVERAGE_Q8.load(Ordering::Relaxed);
            let delta_q8 = (average_q8 - offset_q8).clamp(i16::MIN as i32, i16::MAX as i32);
            let current_ca = CURRENT_CENTIAMPS
                .load(Ordering::Relaxed)
                .clamp(i16::MIN as i32, i16::MAX as i32);
            let current_diagnostic = [
                (offset_q8 >> 8) as u8,
                (offset_q8 >> 16) as u8,
                (average_q8 >> 8) as u8,
                (average_q8 >> 16) as u8,
                delta_q8 as u8,
                (delta_q8 >> 8) as u8,
                current_ca as u8,
                (current_ca >> 8) as u8,
            ];
            let _ = can_data
                .write(&CanFrame::new(
                    CanMsg::CurrentDiagnosticId.as_raw(),
                    &current_diagnostic,
                ))
                .await;

            let status = u8::from(ltc_diagnostics.config_valid)
                | (u8::from(ltc_diagnostics.cell_pec_valid) << 1)
                | (u8::from(ltc_diagnostics.auxa_pec_valid) << 2)
                | (u8::from(ltc_diagnostics.auxb_pec_valid) << 3);
            let ltc_diagnostic = [
                status,
                ltc_diagnostics.cell_group,
                ltc_diagnostics.received_pec as u8,
                (ltc_diagnostics.received_pec >> 8) as u8,
                ltc_diagnostics.calculated_pec as u8,
                (ltc_diagnostics.calculated_pec >> 8) as u8,
                ltc_diagnostics.cell_pec_errors,
                ltc_diagnostics.aux_pec_errors,
            ];
            let _ = can_data
                .write(&CanFrame::new(
                    CanMsg::LtcDiagnosticId.as_raw(),
                    &ltc_diagnostic,
                ))
                .await;

            let adc_raw = (average_q8 >> 8).clamp(0, u16::MAX as i32) as u16;
            let vrefint_raw = ADC_VREFINT_RAW.load(Ordering::Relaxed) as u16;
            let vref_mv = ADC_VREF_MILLIVOLTS.load(Ordering::Relaxed) as u16;
            let input_mv = ADC_INPUT_MILLIVOLTS.load(Ordering::Relaxed) as u16;
            let adc_diagnostic = [
                adc_raw as u8,
                (adc_raw >> 8) as u8,
                vrefint_raw as u8,
                (vrefint_raw >> 8) as u8,
                vref_mv as u8,
                (vref_mv >> 8) as u8,
                input_mv as u8,
                (input_mv >> 8) as u8,
            ];
            let _ = can_data
                .write(&CanFrame::new(
                    CanMsg::AdcDiagnosticId.as_raw(),
                    &adc_diagnostic,
                ))
                .await;

            let register_frames = [
                (
                    CanMsg::AdcRegisterDiagnosticId,
                    (ADC_DR_SNAPSHOT.load(Ordering::Relaxed) & 0xffff)
                        | ((ADC_ISR_SNAPSHOT.load(Ordering::Relaxed) & 0xffff) << 16),
                    (ADC_CR_SNAPSHOT.load(Ordering::Relaxed) & 0xffff)
                        | ((ADC_CFGR_SNAPSHOT.load(Ordering::Relaxed) & 0xffff) << 16),
                ),
                (
                    CanMsg::AdcMuxDiagnosticId,
                    (ADC_SQR1_SNAPSHOT.load(Ordering::Relaxed) & 0xffff)
                        | ((ADC_SMPR1_SNAPSHOT.load(Ordering::Relaxed) & 0xffff) << 16),
                    ADC_CCR_SNAPSHOT.load(Ordering::Relaxed),
                ),
                (
                    CanMsg::AdcCalibrationDiagnosticId,
                    ADC_DIFSEL_SNAPSHOT.load(Ordering::Relaxed),
                    ADC_CALFACT_SNAPSHOT.load(Ordering::Relaxed),
                ),
                (
                    CanMsg::AdcGpioDiagnosticId,
                    ADC_GPIO_MODER_SNAPSHOT.load(Ordering::Relaxed),
                    ADC_GPIO_PUPDR_SNAPSHOT.load(Ordering::Relaxed),
                ),
                (
                    CanMsg::AdcClockDiagnosticId,
                    ADC_SMPR2_SNAPSHOT.load(Ordering::Relaxed),
                    ADC_RCC_CCIPR5_SNAPSHOT.load(Ordering::Relaxed),
                ),
                (
                    CanMsg::AdcOptionDiagnosticId,
                    ADC_OR_SNAPSHOT.load(Ordering::Relaxed),
                    ADC_CFGR2_SNAPSHOT.load(Ordering::Relaxed),
                ),
            ];
            for (id, first, second) in register_frames {
                let payload = [
                    first as u8,
                    (first >> 8) as u8,
                    (first >> 16) as u8,
                    (first >> 24) as u8,
                    second as u8,
                    (second >> 8) as u8,
                    (second >> 16) as u8,
                    (second >> 24) as u8,
                ];
                let _ = can_data.write(&CanFrame::new(id.as_raw(), &payload)).await;
            }
            drop(can_data);
            time_send_log = embassy_time::Instant::now().as_millis();
        }

        drop(bms_data);

        let mut err_check_data = err_check.lock().await;
        if !(fault_temp || fault_volt) {
            if embassy_time::Instant::now().as_millis() > 1000
                && CURRENT_CALIBRATED.load(Ordering::Acquire)
            {
                err_check_data.set_high();
            }
            debug_led.set_high();
        } else {
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
