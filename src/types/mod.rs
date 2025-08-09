pub mod bms;
pub use bms::SLAVEBMS;

#[repr(u16)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CanMsg {
    VoltageId = 0x54,
    TemperatureId = 0x55,
    Balancing = 0x1A4,
    ErrorId = 0x14,
    Tech = 0x365,
    Tech1 = 0x366,
    Tech2 = 0x367,
    Tech3 = 0x368 
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
pub enum TEMPERATURES {
    MAXTEMP = 600,
    MINTEMP = 100
}

impl TEMPERATURES {
    pub fn _as_raw(&self) -> u16 {
        *self as u16
    }
}