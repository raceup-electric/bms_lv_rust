pub use super::CanFrame;

use embassy_stm32::bind_interrupts;
use embassy_stm32::can::config::TxBufferMode;
use embassy_stm32::can::{self, CanConfigurator, CanRx, CanTx};
use embassy_stm32::peripherals::{FDCAN1, PD0, PD1};
use embassy_stm32::Peri;
use embassy_time::{with_timeout, Duration};

bind_interrupts!(struct IrqsFdcan1 {
    FDCAN1_IT0 => can::IT0InterruptHandler<FDCAN1>;
    FDCAN1_IT1 => can::IT1InterruptHandler<FDCAN1>;
});

#[derive(Debug)]
pub enum CanError {
    NoItem,
    Timeout,
    WriteError,
}

pub struct CanController<'a> {
    can: CanTx<'a>,
}

pub struct CanReceiver<'a> {
    can: CanRx<'a>,
}

impl<'a> CanController<'a> {
    pub fn new_fdcan1(
        peri: Peri<'a, FDCAN1>,
        rx: Peri<'a, PD0>,
        tx: Peri<'a, PD1>,
        baudrate: u32,
    ) -> (Self, CanReceiver<'a>) {
        let mut configurator = CanConfigurator::new(peri, rx, tx, IrqsFdcan1);
        configurator.set_bitrate(baudrate);
        let config = configurator
            .config()
            .set_protocol_exception_handling(false)
            .set_tx_buffer_mode(TxBufferMode::Fifo);
        configurator.set_config(config);
        let (tx, rx, _) = configurator.into_normal_mode().split();
        (Self { can: tx }, CanReceiver { can: rx })
    }

    pub async fn write(&mut self, frame: &CanFrame) -> Result<(), CanError> {
        match with_timeout(Duration::from_millis(10), self.can.write(&frame.frame())).await {
            Ok(_) => Ok(()),
            Err(_) => Err(CanError::Timeout),
        }
    }
}

impl CanReceiver<'_> {
    pub async fn read(&mut self) -> Result<CanFrame, CanError> {
        match self.can.read().await {
            Ok(envelope) => Ok(CanFrame::from_envelope(envelope)),
            Err(_) => Err(CanError::NoItem),
        }
    }
}
