pub mod bms;
pub use bms::SLAVEBMS;

#[repr(u16)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CanMsg {
    VoltageId = 0x21C,
    TemperatureId = 0x220,
    Balancing = 0x64,
    ErrorId = 0x5A,
    CurrentId = 0x21E,
    Tech = 0x1F9,
    Tech1 = 0x2F8,
    Tech2 = 0x2F9,
    Tech3 = 0x2FA,
    BMSLVTemps1 = 0x2FB, // 763
    BMSLVTemps2 = 0x2FC, // 764
    BMSLVTemps3 = 0x2FD, // 765

}


impl CanMsg {
    pub fn as_raw(&self) -> u16 {
        *self as u16
    }
}

#[repr(u16)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum VOLTAGES {
    MAXVOLTAGE = 42000,
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
    MAXTEMP = 60,
    MINTEMP = 10
}

impl TEMPERATURES {
    pub fn _as_raw(&self) -> u16 {
        *self as u16
    }
}