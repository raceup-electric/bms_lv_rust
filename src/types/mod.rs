//! Battery Management System (BMS) module for monitoring voltage and temperature.
//!
//! This module defines the communication protocol and constants related to the BMS, including CAN message
//! identifiers, voltage limits, and temperature limits for the system.

pub mod bms;
pub use bms::BMS;

/// Enum representing different CAN message types for the BMS.
#[repr(u16)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CanMsg {
    /// CAN message ID for voltage data.
    VoltageId = 0x57,
    /// CAN message ID for temperature data.
    TemperatureId = 0x58,
}

impl CanMsg {
    /// Converts the `CanMsg` enum to its raw u16 value.
    ///
    /// # Returns
    ///
    /// * `u16` - The raw value of the CAN message ID.
    pub fn as_raw(&self) -> u16 {
        *self as u16
    }
}

/// Enum representing the voltage limits for the BMS.
#[repr(u16)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum VOLTAGES {
    /// Maximum voltage limit for the BMS.
    MAXVOLTAGE = 42800,
    /// Minimum voltage limit for the BMS.
    MINVOLTAGE = 32000,
}

impl VOLTAGES {
    /// Converts the `VOLTAGES` enum to its raw u16 value.
    ///
    /// # Returns
    ///
    /// * `u16` - The raw value of the voltage limit.
    pub fn as_raw(&self) -> u16 {
        *self as u16
    }
}

/// Enum representing temperature limits for the BMS.
#[repr(u16)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum TEMPERATURES {
    // Maximum temperature limit for the BMS.
    MAXTEMP = 60,
    /// Minimum temperature limit for the BMS.
    MINTEMP = 10
}

impl TEMPERATURES {
    /// Converts the `TEMPERATURES` enum to its raw u16 value.
    ///
    /// # Returns
    ///
    /// * `u16` - The raw value of the temperature limit.
    pub fn as_raw(&self) -> u16 {
        *self as u16
    }
}