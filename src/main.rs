//! # BMS LV — application entry point
//!
//! Firmware for the Low-Voltage Battery Management System of the RaceUP
//! Formula SAE car, built on the [Embassy](https://embassy.dev) async runtime.
//!
//! At boot the firmware initialises the STM32F405 peripherals and spawns four
//! cooperative async tasks that share state through `Mutex`-guarded
//! [`StaticCell`]s:
//!
//! * [`current_sense`] — samples the pack current via ADC1 and calibrates it.
//! * [`send_can`]      — periodically broadcasts voltage/temperature/"tech" frames.
//! * [`read_can`]      — receives commands (balancing enable, tech-mode toggle).
//! * [`ltc_function`]  — drives the LTC6811: reads cells/temps, runs the fault
//!   logic, controls balancing, emits the error frame and logs over USB.
//!
//! The pack is a 48 V LV battery of 12 series cells monitored by a single
//! LTC6811 (12 cells + 4 thermistors).

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_stm32::adc::{Adc, Resolution};
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::peripherals::ADC1;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use libm::roundf;
use static_cell::StaticCell;

use crate::usb_serial::usb::Serial;
use crate::{
    can_management::{CanError, CanFrame},
    ltc_management::ltc6811::Mode,
};

use defmt::info;
// use panic_probe as _;

mod can_management;
mod ltc_management;
mod types;
mod usb_serial;

use can_management::{can_operation, can_operation_tech, CanController};
use ltc_management::{SpiDevice, LTC6811};
use types::{CanMsg, SLAVEBMS, TEMPERATURES, VOLTAGES};
use usb_serial::prepare_config;

// ---------------------------------------------------------------------------
// Globally shared state.
//
// Each resource that must be reached from more than one task lives in a
// `StaticCell` (allocated once, `'static` lifetime) wrapped in an async
// `Mutex`. The `CriticalSectionRawMutex` makes the lock usable from any
// execution context on a single-core MCU.
// ---------------------------------------------------------------------------

/// Aggregated battery state (cell voltages, temperatures, current, history).
static BMS: StaticCell<Mutex<CriticalSectionRawMutex, SLAVEBMS>> = StaticCell::new();
/// Hardware "all good" / error line driven toward the safety circuit.
static ERR_CHECK: StaticCell<Mutex<CriticalSectionRawMutex, Output>> = StaticCell::new();
/// CAN controller used by both the TX and RX tasks.
static CAN: StaticCell<Mutex<CriticalSectionRawMutex, CanController>> = StaticCell::new();
/// SPI device connected to the LTC6811.
static SPI: StaticCell<Mutex<CriticalSectionRawMutex, SpiDevice>> = StaticCell::new();
/// LTC6811 driver instance.
static LTC: StaticCell<Mutex<CriticalSectionRawMutex, LTC6811>> = StaticCell::new();
/// Whether cell balancing is currently requested.
static IS_BALANCE: StaticCell<Mutex<CriticalSectionRawMutex, bool>> = StaticCell::new();
/// Whether extended "tech" telemetry is enabled.
static IS_TECH: StaticCell<Mutex<CriticalSectionRawMutex, bool>> = StaticCell::new();

// static TEMP_HC: StaticCell<Mutex<CriticalSectionRawMutex, [u16; 2]>> = StaticCell::new();

/// Zero-current ADC bias of the current sensor, in millivolts.
const VOLTAGE_OFFSET: f32 = 1650f32; //mV

/// Application entry point.
///
/// Initialises the clock tree and peripherals, builds the shared state and
/// spawns every task. The final loop only keeps the executor alive.
#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    // Bring up clocks/peripherals with the project clock configuration.
    let p = embassy_stm32::init(prepare_config());

    // ADC1 + pin used to sense the pack current.
    let current_adc: embassy_stm32::adc::Adc<'static, ADC1> = Adc::new(p.ADC1);
    let current_pin: embassy_stm32::peripherals::PA1 = p.PA1;

    // The CAN controller uses CAN2 on the bus; CAN1 must be enabled as the
    // master instance so the shared filter bank works. `rx1`/`tx1` (the CAN1
    // pins) are returned so they can be reused by the USB driver.
    let (can, rx1, tx1) =
        CanController::new_can2(p.CAN2, p.PB12, p.PB13, 500_000, p.CAN1, p.PA11, p.PA12).await;
    let can_mutex = Mutex::new(can);
    let can = StaticCell::init(&CAN, can_mutex);

    // Bring up the USB-CDC serial used as the log sink.
    Serial::init(p.USB_OTG_FS, tx1, rx1, &spawner);

    // Status LEDs.
    let debug_led = Output::new(p.PC13, Level::Low, Speed::High);
    let temp_led = Output::new(p.PC9, Level::Low, Speed::High);
    let voltage_led = Output::new(p.PC11, Level::Low, Speed::High);

    // Hardware error/heartbeat line toward the shutdown circuit.
    let err_check = Output::new(p.PA2, Level::Low, Speed::High);
    let err_check_mutex = Mutex::new(err_check);
    let err_check = StaticCell::init(&ERR_CHECK, err_check_mutex);

    // Balancing starts disabled.
    let is_balance = false;
    let is_balance_mutex = Mutex::new(is_balance);
    let is_balance = StaticCell::init(&IS_BALANCE, is_balance_mutex);

    // Extended telemetry starts enabled.
    let is_tech = true;
    let is_tech_mutex = Mutex::new(is_tech);
    let is_tech = StaticCell::init(&IS_TECH, is_tech_mutex);

    // Shared battery state.
    let bms = setup_bms();
    let bms_mutex = Mutex::new(bms);
    let bms = StaticCell::init(&BMS, bms_mutex);

    // Spawn current sensing and CAN transmit tasks.
    spawner
        .spawn(current_sense(current_adc, current_pin, bms))
        .unwrap();
    spawner.spawn(send_can(bms, can, is_tech)).unwrap();

    //info!("Hello world over USB-CDC!");

    // Bring up SPI1 (+ DMA) and the LTC6811 driver on top of it.
    let spi: SpiDevice<'static> =
        SpiDevice::new(p.SPI1, p.PA5, p.PA7, p.PA6, p.PA4, p.DMA2_CH3, p.DMA2_CH0).await;
    let spi_mutex = Mutex::new(spi);
    let spi = StaticCell::init(&SPI, spi_mutex);

    let mut ltc = LTC6811::new(spi, bms).await; // Initialize LTC6811
    match ltc.init().await {
        Ok(_) => {} //info!("LTC6811 initialized successfully"),
        Err(_) => defmt::error!("Failed to initialize LTC6811"),
    }

    let ltc_mutex = Mutex::new(ltc);
    let ltc = StaticCell::init(&LTC, ltc_mutex);
    // Main acquisition / fault / balancing task.
    spawner
        .spawn(ltc_function(
            bms,
            ltc,
            err_check,
            can,
            debug_led,
            voltage_led,
            temp_led,
            is_balance,
        ))
        .unwrap();

    // CAN receive task.
    spawner.spawn(read_can(is_balance, can, is_tech)).unwrap();

    // Idle loop: the real work happens in the spawned tasks.
    loop {
        embassy_time::Timer::after_millis(10000).await;
    }
}

/// Build the initial (zeroed) battery state.
fn setup_bms() -> SLAVEBMS {
    let bms = SLAVEBMS::new();
    bms
}

/// Continuously samples the pack current on ADC1.
///
/// The routine first measures the sensor's zero-current bias, derives a scaling
/// factor, then in steady state averages 50 samples, converts to a signed
/// current and stores it in the shared [`SLAVEBMS`].
#[embassy_executor::task]
async fn current_sense(
    mut adc: embassy_stm32::adc::Adc<'static, ADC1>,
    mut curr_pin: embassy_stm32::peripherals::PA1,
    bms: &'static Mutex<CriticalSectionRawMutex, SLAVEBMS>,
) {
    adc.set_resolution(Resolution::BITS12);
    embassy_time::Timer::after_millis(100).await;

    // --- Calibration: average 10 samples to estimate the no-current offset ---
    let mut count: u64 = 0;
    for _ in 0..10 {
        count = count.wrapping_add(adc.blocking_read(&mut curr_pin) as u64);
        embassy_time::Timer::after_millis(1).await;
    }

    // Convert the averaged ADC code to millivolts (12-bit, 3.3 V reference).
    let no_current_offset = ((count as f32) / 10.0f32) * 3300f32 / (4095 as f32);
    let factor = no_current_offset / VOLTAGE_OFFSET;

    // --- Steady-state acquisition loop ---
    loop {
        count = 0;
        for _ in 0..50 {
            count = count.wrapping_add(adc.blocking_read(&mut curr_pin) as u64);
            embassy_time::Timer::after_micros(200).await;
        }

        // Average -> mV, subtract bias, scale to a current reading.
        let mut f_curr = ((count as f32) / 50.0f32) * 3300f32 / (4095 as f32);
        f_curr = ((f_curr - no_current_offset) / (9.2f32 * factor)) * 10000f32;

        // Round toward zero on each side so noise around 0 stays at 0.
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

/// Periodically broadcasts the pack state on the CAN bus.
///
/// Always sends the summary voltage/temperature frames; when "tech" mode is
/// enabled it additionally sends the per-cell telemetry frames.
#[embassy_executor::task]
async fn send_can(
    bms: &'static Mutex<CriticalSectionRawMutex, SLAVEBMS>,
    can: &'static Mutex<CriticalSectionRawMutex, CanController<'static>>,
    is_tech: &'static Mutex<CriticalSectionRawMutex, bool>,
) {
    loop {
        // Summary frames (max/min/avg/total voltage, temps, current).
        let bms_data = bms.lock().await;
        let mut can_data = can.lock().await;
        match can_operation(&bms_data, &mut can_data).await {
            Ok(_) => {}
            Err(_) => {}
        }
        drop(can_data);
        drop(bms_data);
        embassy_time::Timer::after_millis(10).await;

        // Snapshot the tech flag (released before re-locking the bus/state).
        let is_tech_data = is_tech.lock().await;
        let tech: bool = *is_tech_data;
        drop(is_tech_data);
        embassy_time::Timer::after_millis(1).await;

        // Optional extended per-cell telemetry.
        if tech == true {
            let bms_data = bms.lock().await;
            let mut can_data = can.lock().await;
            match can_operation_tech(&bms_data, &mut can_data).await {
                Ok(_) => {}
                Err(_) => {}
            }
            drop(can_data);
            drop(bms_data);
        }
        embassy_time::Timer::after_millis(189).await;
    }
}

/// Receives CAN commands and updates the shared control flags.
///
/// Recognised messages:
/// * [`CanMsg::Balancing`] — first byte non-zero enables balancing.
/// * [`CanMsg::Tech`]      — first byte non-zero enables tech telemetry.
#[embassy_executor::task]
async fn read_can(
    is_balance: &'static Mutex<CriticalSectionRawMutex, bool>,
    can: &'static Mutex<CriticalSectionRawMutex, CanController<'static>>,
    is_tech: &'static Mutex<CriticalSectionRawMutex, bool>,
) {
    loop {
        let mut can_data = can.lock().await;
        match can_data.read().await {
            Ok(frame) => {
                let id = frame.id();
                let bytes = frame.bytes();
                drop(can_data); // release the bus before touching the flags
                                // Balancing enable/disable.
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
                // Tech-telemetry enable/disable.
                if id == CanMsg::Tech.as_raw() {
                    if bytes[0] >= 0x1 as u8 {
                        let mut is_tech_data = is_tech.lock().await;
                        *is_tech_data = true;
                        drop(is_tech_data);
                    } else if bytes[0] == 0x0 as u8 {
                        let mut is_tech_data = is_tech.lock().await;
                        *is_tech_data = false;
                        drop(is_tech_data);
                    }
                }
            }
            Err(_) => {
                drop(can_data); // nothing to read this cycle
            }
        }
        embassy_time::Timer::after_micros(50).await;
    }
}

/// Core BMS task: acquisition, fault detection, balancing and signalling.
///
/// Each iteration:
/// 1. Refreshes cell voltages and temperatures from the LTC6811.
/// 2. Flags over/under-voltage and over/under-temperature with ~450 ms
///    debouncing, lighting the relevant status LED.
/// 3. Drives the hardware error line and broadcasts an error CAN frame when a
///    fault is latched.
/// 4. Periodically logs every cell voltage and temperature over USB.
/// 5. Runs passive balancing windows while balancing is requested.
#[embassy_executor::task]
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
    // Debounce timestamps and latched fault flags.
    let mut time_err_volt = embassy_time::Instant::now().as_millis();
    let mut time_err_temp = embassy_time::Instant::now().as_millis();
    let mut fault_temp: bool = false;
    let mut fault_volt: bool = false;
    let mut first_close = false;

    let mut time_send_log = embassy_time::Instant::now().as_millis();

    loop {
        // --- Refresh measurements ---
        let mut ltc_data = ltc.lock().await;

        match ltc_data.update().await {
            Ok(_) => {}
            Err(_) => {
                defmt::error!("Failed to update battery data");
            }
        }

        // While balancing, take extra readings to keep data fresh during the
        // discharge windows.
        let is_balance_data = is_balance.lock().await;
        let balance: bool = *is_balance_data;
        drop(is_balance_data);
        if balance == true {
            for _ in 0..5 {
                match ltc_data.update().await {
                    Ok(_) => {}
                    Err(_) => {
                        defmt::error!("Failed to update battery data");
                    }
                }
            }
        }

        drop(ltc_data);

        // --- Fault evaluation (with ~450 ms debounce) ---
        let bms_data = bms.lock().await;
        if &bms_data.min_volt() < &VOLTAGES::MINVOLTAGE.as_raw()
            || &bms_data.max_volt() > &VOLTAGES::MAXVOLTAGE.as_raw()
        {
            if embassy_time::Instant::now().as_millis() - time_err_volt > 450 {
                voltage_led.set_high();
                fault_volt = true;
            }
        } else {
            fault_volt = false;
            first_close = true;
            time_err_volt = embassy_time::Instant::now().as_millis();
        }

        if &bms_data.min_temp() < &TEMPERATURES::MINTEMP._as_raw()
            || &bms_data.max_temp() > &TEMPERATURES::MAXTEMP._as_raw()
        {
            if embassy_time::Instant::now().as_millis() - time_err_temp > 450 {
                temp_led.set_high();
                fault_temp = false;
            }
        } else {
            fault_temp = false;
            first_close = true;
            time_err_temp = embassy_time::Instant::now().as_millis();
            temp_led.set_low();
        }

        // --- Periodic per-cell / per-thermistor logging (once per second) ---
        if embassy_time::Instant::now().as_millis() - time_send_log > 1000 {
            for i in 0..12 {
                info!(
                    "Cell {}: {} mV",
                    i,
                    roundf(bms_data.cell_volts(i) as f32 / 10f32)
                );
                embassy_time::Timer::after_millis(1).await;
            }

            for i in 0..4 {
                info!("Temp {}: {} C", i, roundf(bms_data.temps(i) as f32 / 10f32));
                embassy_time::Timer::after_millis(1).await;
            }

            info!(
                "Fault Temp: {}\nFault Cells: {}",
                if fault_temp { "YES" } else { "NO" },
                if fault_volt { "YES" } else { "NO" }
            );
            embassy_time::Timer::after_millis(2).await;
            time_send_log = embassy_time::Instant::now().as_millis();
        }

        drop(bms_data);

        // --- Drive the hardware error line / error CAN frame ---
        let mut err_check_data = err_check.lock().await;
        if !(fault_temp || fault_volt) {
            // Healthy: assert the "all good" line after a short startup delay.
            if embassy_time::Instant::now().as_millis() > 1000 {
                err_check_data.set_high();
            }
            debug_led.set_low();
        } else {
            // Faulted: release the line and broadcast an error frame.
            err_check_data.set_low();
            if embassy_time::Instant::now().as_millis() > 2000 || first_close {
                debug_led.toggle();
                let mut can_data = can.lock().await;
                let can_second = [1];

                let frame_send = CanFrame::new(CanMsg::ErrorId.as_raw(), &can_second);
                match can_data.write(&frame_send).await {
                    Ok(_) => {}

                    Err(CanError::Timeout) => {
                        // info!("Timeout Can connection");
                    }

                    Err(_) => {
                        // info!("Can write error");
                    }
                }
                drop(can_data);
                embassy_time::Timer::after_millis(200).await;
            }
        }
        drop(err_check_data);

        // --- Balancing window ---
        let mut is_balance_data = is_balance.lock().await;
        let balance: bool = *is_balance_data;
        if balance == true {
            let mut ltc_data = ltc.lock().await;
            // Stop balancing once no cell is above the min by more than epsilon.
            if !ltc_data.check_need_balance().await {
                *is_balance_data = false;
            }
            // Keep the LTC6811 in balancing mode for ~10 s.
            let time = embassy_time::Instant::now().as_millis();
            while embassy_time::Instant::now().as_millis() - time < 10000 {
                ltc_data.set_mode(Mode::BALANCING).await;
                embassy_time::Timer::after_millis(5).await;
            }
            drop(ltc_data);
        } else {
            embassy_time::Timer::after_millis(5).await;
        }

        drop(is_balance_data);
        // info!("ALIVE");
        embassy_time::Timer::after_millis(5).await;
    }
}
