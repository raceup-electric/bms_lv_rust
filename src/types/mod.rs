pub mod bms;
pub use bms::SLAVEBMS;

#[repr(u16)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CanMsg {
    VoltageId = 0x54,
    TemperatureId = 0x55,
    Balancing = 0x1A4,
    ErrorId = 0x14
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
    MINVOLTAGE = 33000
}

impl VOLTAGES {
    pub fn as_raw(&self) -> u16 {
        *self as u16
    }
}

#[repr(u16)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum _TEMPERATURES {
    _MAXTEMP = 58,
    _MINTEMP = 10
}

impl _TEMPERATURES {
    pub fn _as_raw(&self) -> u16 {
        *self as u16
    }
}