pub mod bms;
pub use bms::BMS;

#[repr(u16)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CanMsg {
    VoltageId = 0x57,
    TemperatureId = 0x58
}

impl CanMsg {
    pub fn as_raw(&self) -> u16 {
        *self as u16
    }
}

#[repr(u16)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum VOLTAGES {
    MAXVOLTAGE = 42800,
    MINVOLTAGE = 32000
}

impl VOLTAGES {
    pub fn as_raw(&self) -> u16 {
        *self as u16
    }
}

#[repr(u16)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum _TEMPERATURES {
    _MAXTEMP = 60,
    _MINTEMP = 10
}

impl _TEMPERATURES {
    pub fn _as_raw(&self) -> u16 {
        *self as u16
    }
}