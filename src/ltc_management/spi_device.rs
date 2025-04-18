//! SPI Device module for configuring and interacting with SPI devices using STM32 peripherals.
//!
//! This module provides the `SpiDevice` struct, which wraps around an SPI interface to manage
//! communication with SPI peripherals, including supporting read, write, and transfer operations.

use embassy_stm32::gpio::{Level, Output, Pin, Speed};
use embassy_stm32::mode::Async;
use embassy_stm32::spi::{BitOrder, Config, Instance, MisoPin, MosiPin, RxDma, SckPin, Spi, TxDma, MODE_3};
use embassy_stm32::time::Hertz;
use embassy_stm32::Peripheral;

use {defmt_rtt as _, panic_probe as _};

/// Struct representing an SPI device interface.
///
/// This struct wraps the STM32 SPI peripheral and allows for basic SPI operations such as
/// writing, reading, and bidirectional transfers.
pub struct SpiDevice<'a> {
    /// The SPI instance used for communication.
    spi: Option<Spi<'a, Async>>,
    /// Chip select pin for the SPI device.
    pub cs: Output<'a>,
}

impl<'a> SpiDevice<'a> {
    /// Creates a new SPI device instance with the specified pins and DMA settings.
    ///
    /// # Arguments
    ///
    /// * `peri` - The SPI peripheral instance.
    /// * `sck` - The clock (SCK) pin for SPI.
    /// * `mosi` - The master-out slave-in (MOSI) pin for SPI.
    /// * `miso` - The master-in slave-out (MISO) pin for SPI.
    /// * `cs` - The chip select (CS) pin for SPI.
    /// * `tx_dma` - The DMA channel for SPI transmit.
    /// * `rx_dma` - The DMA channel for SPI receive.
    ///
    /// # Returns
    ///
    /// Returns a new `SpiDevice` instance.
    pub async fn new<T: Instance>(
        peri: impl Peripheral<P = T> + 'a, 
        sck: impl Peripheral<P = impl SckPin<T>> + 'a, 
        mosi: impl Peripheral<P = impl MosiPin<T>> + 'a, 
        miso: impl Peripheral<P = impl MisoPin<T>> + 'a, 
        cs: impl Peripheral<P = impl Pin> + 'a,
        tx_dma: impl Peripheral<P = impl TxDma<T>> + 'a,
        rx_dma: impl Peripheral<P = impl RxDma<T>> + 'a,
    ) -> Self {

        let mut spi_config = Config::default();
        spi_config.mode = MODE_3;
        spi_config.bit_order = BitOrder::MsbFirst;
        spi_config.frequency = Hertz(500_000);
        
        let spi = Spi::new( 
            peri,
            sck,
            mosi,
            miso,
            tx_dma,
            rx_dma,
            spi_config
        );
        
        let spi_device = SpiDevice { 
            spi: Some(spi),
            cs: Output::new(cs, Level::High, Speed::Medium),
        };

        spi_device
    }

    /// Writes data to the SPI device.
    ///
    /// This method will perform a write operation to the SPI device using the provided data buffer.
    /// The chip select (CS) pin is asserted before the write and deasserted after the write is complete.
    ///
    /// # Arguments
    ///
    /// * `data` - A slice of bytes to write to the SPI device.
    ///
    /// # Errors
    ///
    /// If the SPI write operation fails, an error will be logged.
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
    
    /// Reads data from the SPI device into the provided buffer.
    ///
    /// This method will perform a read operation from the SPI device into the provided buffer.
    /// The chip select (CS) pin is asserted before the read and deasserted after the read is complete.
    ///
    /// # Arguments
    ///
    /// * `buffer` - A mutable slice of bytes to store the received data.
    ///
    /// # Errors
    ///
    /// If the SPI read operation fails, an error will be logged.
    pub async fn _read(&mut self, buffer: &mut [u8]) {
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

    /// Transfers data to and from the SPI device (full-duplex).
    ///
    /// This method will transfer data from the `tx_buffer` to the SPI device while simultaneously
    /// receiving data into the `rx_buffer`. The chip select (CS) pin is asserted before the transfer
    /// and deasserted after the transfer is complete.
    ///
    /// # Arguments
    ///
    /// * `tx_buffer` - A slice of bytes to transmit to the SPI device.
    /// * `rx_buffer` - A mutable slice of bytes to receive the data from the SPI device.
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the transfer was successful.
    /// * `Err(())` if the transfer failed.
    pub async fn transfer(&mut self, tx_buffer: &[u8], rx_buffer: &mut [u8]) -> Result<(), ()> {
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
}
