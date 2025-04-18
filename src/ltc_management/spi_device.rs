use embassy_stm32::gpio::{Level, Output, Pin, Speed};
use embassy_stm32::mode::Async;
use embassy_stm32::spi::{BitOrder, Config, Instance, MisoPin, MosiPin, RxDma, SckPin, Spi, TxDma, MODE_3};
use embassy_stm32::time::Hertz;
use embassy_stm32::Peripheral;
use {defmt_rtt as _, panic_probe as _};

pub struct SpiDevice<'a> {
    spi: Option<Spi<'a, Async>>,
    cs: Output<'a>
}

impl<'a> SpiDevice<'a> {
    pub async fn new<T: Instance>(
        peri: (impl Peripheral<P = T> + 'a), 
        sck: (impl Peripheral<P = impl SckPin<T>> + 'a), 
        mosi: (impl Peripheral<P = impl MosiPin<T>> + 'a), 
        miso: (impl Peripheral<P = impl MisoPin<T>> + 'a), 
        cs:  (impl Peripheral<P = impl Pin> + 'a),
        tx_dma: (impl Peripheral<P = impl TxDma<T>> + 'a),
        rx_dma: (impl Peripheral<P = impl RxDma<T>> + 'a)
    ) -> Self {

        let mut spi_config = Config::default();
        spi_config.mode = MODE_3;
        spi_config.bit_order = BitOrder::MsbFirst;
        spi_config.frequency = Hertz(1_000_000);
        
        let spi = Spi::new(
            peri,
            sck,
            mosi,
            miso,
            tx_dma,
            rx_dma,
            spi_config
        );
        
        SpiDevice { 
            spi: Some(spi),
            cs: Output::new(cs, Level::High, Speed::VeryHigh)
        }
    }

    pub async fn write(&mut self, data: &[u8]) {
        if let Some(spi) = self.spi.as_mut() {
            self.cs.set_low();
    
            spi.write(data).await.unwrap();
    
            self.cs.set_high();
        } else {
            panic!("SPI instance not initialized");
        }
    }
    
    pub async fn read(&mut self, buffer: &mut [u8]) {
        // Ensure spi is initialized before using
        if let Some(spi) = self.spi.as_mut() {
            self.cs.set_low();
    
            spi.read(buffer).await.unwrap();
    
            self.cs.set_high();
        } else {
            panic!("SPI instance not initialized");
        }
    }
}