use embassy_stm32::dma::InterruptHandler;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::interrupt::typelevel::Binding;
use embassy_stm32::mode::Async;
use embassy_stm32::peripherals::{GPDMA1_CH0, GPDMA1_CH1, PA4, PA5, PA6, PA7, SPI1};
use embassy_stm32::spi::{mode::Master, BitOrder, Config, Spi, MODE_3};
use embassy_stm32::time::Hertz;
use embassy_stm32::Peri;

pub struct SpiDevice<'a> {
    spi: Option<Spi<'a, Async, Master>>,
    pub cs: Output<'a>,
}

impl<'a> SpiDevice<'a> {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        peri: Peri<'a, SPI1>,
        sck: Peri<'a, PA5>,
        mosi: Peri<'a, PA7>,
        miso: Peri<'a, PA6>,
        cs: Peri<'a, PA4>,
        tx_dma: Peri<'a, GPDMA1_CH0>,
        rx_dma: Peri<'a, GPDMA1_CH1>,
        irqs: impl Binding<
                embassy_stm32::interrupt::typelevel::GPDMA1_CHANNEL0,
                InterruptHandler<GPDMA1_CH0>,
            > + Binding<
                embassy_stm32::interrupt::typelevel::GPDMA1_CHANNEL1,
                InterruptHandler<GPDMA1_CH1>,
            > + 'a,
    ) -> Self {
        let mut spi_config = Config::default();
        spi_config.mode = MODE_3;
        spi_config.bit_order = BitOrder::MsbFirst;
        spi_config.frequency = Hertz(1_000_000);

        let spi = Spi::new(peri, sck, mosi, miso, tx_dma, rx_dma, irqs, spi_config);

        let spi = SpiDevice {
            spi: Some(spi),
            cs: Output::new(cs, Level::High, Speed::VeryHigh),
        };

        spi
    }

    pub async fn write(&mut self, data: &[u8]) {
        if let Some(spi) = self.spi.as_mut() {
            self.cs.set_low();
            let _ = spi.write(data).await;
            self.cs.set_high();
        }
    }

    pub async fn cmd_read(&mut self, cmd: &[u8; 4], resp: &mut [u8; 8]) -> Result<(), ()> {
        let spi = self.spi.as_mut().unwrap();

        self.cs.set_low();

        spi.write(cmd).await.map_err(|_| ())?;

        let tx = [0xFFu8; 8];
        spi.transfer(resp, &tx).await.map_err(|_| ())?;

        self.cs.set_high();

        Ok(())
    }

    pub async fn start_comm(&mut self, cmd: &[u8; 4]) -> Result<(), ()> {
        let spi = self.spi.as_mut().ok_or(())?;
        self.cs.set_low();
        let result = match spi.write(cmd).await {
            Ok(()) => spi.write(&[0xFFu8; 9]).await.map_err(|_| ()),
            Err(_) => Err(()),
        };
        self.cs.set_high();
        result
    }
}
