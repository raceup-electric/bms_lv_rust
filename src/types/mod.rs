pub mod bms;
pub use bms::BMS;

#[repr(u16)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CanMsg {
    VoltageId = 0x54,
    TemperatureId = 0x55
}

#[repr(u16)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Voltages {
    MaxVoltage,
    MinVoltage
}

impl CanMsg {
    pub fn as_raw(&self) -> u16 {
        *self as u16
    }
}

impl Voltages {
    pub fn as_raw(&self) -> u16 {
        *self as u16
    }
}