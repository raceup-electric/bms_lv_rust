pub use super::CanFrame;

use embassy_stm32::bind_interrupts;
use embassy_stm32::can::filter::Mask32;
use embassy_stm32::can::{
    Can, Fifo, Rx0InterruptHandler, Rx1InterruptHandler, SceInterruptHandler, TxInterruptHandler
};

use embassy_stm32::peripherals::{CAN1, CAN2, PA11, PA12, PB12, PB13};
use embassy_time::Duration;

bind_interrupts!(struct Irqs1 {
    CAN1_RX0 => Rx0InterruptHandler<CAN1>;
    CAN1_RX1 => Rx1InterruptHandler<CAN1>;
    CAN1_SCE => SceInterruptHandler<CAN1>;
    CAN1_TX => TxInterruptHandler<CAN1>;
});

bind_interrupts!(struct Irqs2 {
    CAN2_RX0 => Rx0InterruptHandler<CAN2>;
    CAN2_RX1 => Rx1InterruptHandler<CAN2>;
    CAN2_SCE => SceInterruptHandler<CAN2>;
    CAN2_TX => TxInterruptHandler<CAN2>;
});


#[derive(Debug)]
pub enum CanError {
    NoItem,
    Timeout,
    WriteError,
}

pub struct CanController<'a> {
    can: Can<'a>,
    tx_frame: Option<CanFrame>
}


impl<'a> CanController<'a>{
    async fn new(mut controller: CanController<'a>, baudrate: u32) -> Self {
        controller.can.modify_config()
            .set_loopback(true) // Receive own frames
            .set_silent(true)
            .set_bitrate(baudrate);
        
        controller.can.modify_filters().enable_bank(0, Fifo::Fifo0, Mask32::accept_all());
        controller.can.enable().await;
        controller
    }

    pub async fn _new_can1(peri: CAN1, rx: PA11, tx: PA12, baudrate: u32) -> Self {
        let controller = CanController {
            can: Can::new(
                peri,
                rx,
                tx,
                Irqs1
            ),
            tx_frame: None
        };
        Self::new(controller, baudrate).await
    }

    pub async fn new_can2(peri: CAN2, rx: PB12, tx: PB13, baudrate: u32) -> Self {
        let controller = CanController {
            can: Can::new(peri, rx, tx, Irqs2),
            tx_frame: None
        };
        Self::new(controller, baudrate).await        
    }

    pub async fn write(&mut self, frame: &CanFrame) -> Result<(), CanError> {
        let mut attempts: u8 = 0;

        while (self.tx_frame.is_some()) && (attempts < 5) {
            embassy_time::Timer::after(Duration::from_millis(10)).await;
            attempts = attempts.wrapping_add(1);
        }

        if attempts >= 5 {
            return Err(CanError::Timeout)
        }

        let new_frame = frame.clone();

        self.tx_frame = Some(new_frame);

        attempts = 0;

        while attempts < 4 {
            if let Some(ref tx_frame) = self.tx_frame {
                match self.can.try_write(&tx_frame.frame()) {
                    Ok(_) => {
                        self.tx_frame = None;
                        return Ok(())
                    }
                    Err(_) => {
                        attempts = attempts.wrapping_add(1);
                    }
                }
            }
        }
        self.tx_frame = None;
        Err(CanError::WriteError)
    } 

    pub async fn read(&mut self) -> Result<CanFrame, CanError> {
        let envelope = self.can.try_read();
        match envelope {
            Ok(_) => {
                let frame = CanFrame::from_envelope(envelope.unwrap());
                return Ok(frame);        
            }

            Err(_) => {
                return Err(CanError::NoItem);
            }
        }
    }
}