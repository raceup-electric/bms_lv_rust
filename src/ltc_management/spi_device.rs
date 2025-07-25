use embassy_stm32::gpio::{Level, Output, Pin, Speed};
use embassy_stm32::mode::Async;
use embassy_stm32::spi::{BitOrder, Config, Instance, MisoPin, MosiPin, RxDma, SckPin, Spi, TxDma, MODE_3};
use embassy_stm32::time::Hertz;
use embassy_stm32::Peripheral;

pub struct SpiDevice<'a> {
    spi: Option<Spi<'a, Async>>,
    pub cs: Output<'a>
}

impl<'a> SpiDevice<'a> {
    pub async fn new<T: Instance>(
        peri: (impl Peripheral<P = T> + 'a), 
        sck: (impl Peripheral<P = impl SckPin<T>> + 'a), 
        mosi: (impl Peripheral<P = impl MosiPin<T>> + 'a), 
        miso: (impl Peripheral<P = impl MisoPin<T>> + 'a), 
        cs:  (impl Peripheral<P = impl Pin> + 'a),
        tx_dma: (impl Peripheral<P = impl TxDma<T>> + 'a),
        rx_dma: (impl Peripheral<P = impl RxDma<T>> + 'a),
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
        
        let spi = SpiDevice { 
            spi: Some(spi),
            cs: Output::new(cs, Level::High, Speed::VeryHigh)
        };

        spi
    }

    pub async fn write(&mut self, data: &[u8]) {
        if let Some(spi) = self.spi.as_mut() {
            self.cs.set_low();
    
            if let Err(e) = spi.write(data).await {
                defmt::error!("SPI write failed: {:?}", e);
            }
            self.cs.set_high();
        } else {
            return;
        }
    }
    
    pub async fn _read(&mut self, buffer: &mut [u8]) {
        // Ensure spi is initialized before using
        if let Some(spi) = self.spi.as_mut() {
            self.cs.set_low();
    
            if let Err(e) = spi.read(buffer).await {
                defmt::error!("SPI read failed: {:?}", e);
            }

            self.cs.set_high();
        } else {
            return;
        }
    }

    pub async fn _transfer(&mut self, tx_buffer: &[u8], rx_buffer: &mut [u8]) -> Result<(), ()> {
        // Ensure spi is initialized before using
        if let Some(spi) = self.spi.as_mut() {
            self.cs.set_low();
            match spi.transfer(rx_buffer, tx_buffer).await {
                Ok(_) => {
                    self.cs.set_high();
                    return Ok(())
                }
                Err(_) => {
                    self.cs.set_high();
                    return Err(());
                }
            }
        } else {
            return Err(());
        }
    }

    pub async fn cmd_read(
        &mut self,
        cmd: &[u8;4],
        resp: &mut [u8;8],
    ) -> Result<(), ()> {
        // take the inner Spi rather than using write()/transfer()
        let spi = self.spi.as_mut().unwrap();

        // 1) CS low once
        self.cs.set_low();

        // 2) send the 4-byte command
        spi.write(cmd).await.map_err(|_| ())?;

        // 3) clock out 8 dummy bytes and capture the response
        let tx = [0xFFu8; 8];
        spi.transfer(resp, &tx).await.map_err(|_| ())?;

        // 4) CS high
        self.cs.set_high();

        Ok(())
    }
}