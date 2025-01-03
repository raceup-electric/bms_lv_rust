use embassy_stm32::gpio::{Level, Output, Pin, Speed};
use embassy_stm32::mode::Blocking;
use embassy_stm32::spi::{BitOrder, Config, Instance, MisoPin, MosiPin, SckPin, Spi, MODE_3};
use embassy_stm32::time::Hertz;
use embassy_stm32::Peripheral;
use {defmt_rtt as _, panic_probe as _};

pub struct SpiDevice<'a> {
    spi: Option<Spi<'a, Blocking>>,
    cs: Output<'a>
}

impl<'a> SpiDevice<'a> {
    pub async fn new<T: Instance>(
        peri: (impl Peripheral<P = T> + 'a), 
        sck: (impl Peripheral<P = impl SckPin<T>> + 'a), 
        mosi: (impl Peripheral<P = impl MosiPin<T>> + 'a), 
        miso: (impl Peripheral<P = impl MisoPin<T>> + 'a), 
        cs:  (impl Peripheral<P = impl Pin> + 'a)
    ) -> Self {

        let mut spi_config = Config::default();
        spi_config.mode = MODE_3;
        spi_config.bit_order = BitOrder::MsbFirst;
        spi_config.frequency = Hertz(1_000_000);
        
        let spi = Spi::new_blocking(
            peri,
            sck,
            mosi,
            miso,
            spi_config
        );
        
        SpiDevice { 
            spi: Some(spi),
            cs: Output::new(cs, Level::High, Speed::VeryHigh)
        }
    }

    pub async fn write() {
        
    }
}