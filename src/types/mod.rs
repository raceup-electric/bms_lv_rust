//! Shared data types and protocol constants.
//!
//! This module re-exports the [`SLAVEBMS`] battery model and defines the
//! enumerations used throughout the firmware: CAN message identifiers and the
//! voltage/temperature protection thresholds.

pub mod bms;
pub use bms::SLAVEBMS;

/// CAN message identifiers used by the BMS.
///
/// Values are the raw 11-bit standard CAN IDs. TX frames carry the pack state
/// (`VoltageId`, `TemperatureId`, the `Tech*` telemetry, `ErrorId`); RX frames
/// carry commands (`Balancing`, `Tech`).
#[repr(u16)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CanMsg {
    /// Summary voltage frame (max/min/avg/total).
    VoltageId = 0x54,
    /// Summary temperature + current frame.
    TemperatureId = 0x55,
    /// Incoming balancing enable/disable command.
    Balancing = 0x1A4,
    /// Outgoing fault/error indication.
    ErrorId = 0x14,
    /// Incoming tech-mode toggle command.
    Tech = 0x365,
    /// Extended telemetry: cells 1-4.
    Tech1 = 0x366,
    /// Extended telemetry: cells 5-8.
    Tech2 = 0x367,
    /// Extended telemetry: cells 9-12.
    Tech3 = 0x368,
    /// Extended telemetry: thermistor temperatures.
    Tech4 = 0x369
}

impl CanMsg {
    /// Returns the raw 11-bit CAN identifier.
    pub fn as_raw(&self) -> u16 {
        *self as u16
    }
}

/// Per-cell voltage protection thresholds, in units of 0.1 mV.
///
/// `42000` -> 4.2000 V (over-voltage), `30000` -> 3.0000 V (under-voltage).
#[repr(u16)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum VOLTAGES {
    /// Cell over-voltage limit (4.2 V).
    MAXVOLTAGE = 42000,
    /// Cell under-voltage limit (3.0 V).
    MINVOLTAGE = 30000
}

impl VOLTAGES {
    /// Returns the threshold as a raw `u16`.
    pub fn as_raw(&self) -> u16 {
        *self as u16
    }
}

/// Temperature protection thresholds (raw sensor scale, 0.1 °C units).
#[repr(u16)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum TEMPERATURES {
    /// Over-temperature limit.
    MAXTEMP = 65000,
    /// Under-temperature limit.
    MINTEMP = 0
}

impl TEMPERATURES {
    /// Returns the threshold as a raw `u16`.
    pub fn _as_raw(&self) -> u16 {
        *self as u16
    }
}
