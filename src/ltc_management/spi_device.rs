use cortex_m_rt::entry;
use defmt::*;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::mode::Blocking;
use embassy_stm32::spi::{Config, Spi};
use embassy_stm32::time::Hertz;
use {defmt_rtt as _, panic_probe as _};

pub struct SpiDevice<'a> {
    spi: Option<Spi<'a, Blocking>>
}

impl<'a> SpiDevice<'a> {
    pub fn new(p: &'a mut embassy_stm32::Peripherals) -> Self {
        let mut spi_config = Config::default();
        spi_config.frequency = Hertz(1_000_000);
        
        let spi = Spi::new_blocking(
            &mut p.SPI3,
            &mut p.PC10,
            &mut p.PC12,
            &mut p.PC11,
            spi_config
        );
        
        SpiDevice { 
            spi: Some(spi)
        }
    }
}