//! LTC6811 battery-monitor layer.
//!
//! Re-exports the low-level [`SpiDevice`] (SPI1 + DMA + chip-select) and the
//! high-level [`LTC6811`] driver that reads cell voltages and temperatures,
//! manages the configuration registers and controls passive balancing.

pub mod spi_device;
pub use spi_device::SpiDevice;
pub mod ltc6811;
pub use ltc6811::LTC6811;
