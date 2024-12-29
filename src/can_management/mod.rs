pub mod frame;

pub use frame::CanFrame;

use embassy_stm32::bind_interrupts;
use embassy_stm32::can::filter::Mask32;
use embassy_stm32::can::{
    Can, Fifo, Rx0InterruptHandler, Rx1InterruptHandler, SceInterruptHandler, TxInterruptHandler,
};

use embassy_stm32::peripherals::CAN1;
use embassy_time::Duration;

bind_interrupts!(struct Irqs {
    CAN1_RX0 => Rx0InterruptHandler<CAN1>;
    CAN1_RX1 => Rx1InterruptHandler<CAN1>;
    CAN1_SCE => SceInterruptHandler<CAN1>;
    CAN1_TX => TxInterruptHandler<CAN1>;
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
    pub async fn new(p: &'a mut embassy_stm32::Peripherals, baudrate: u32) -> Self {
        let mut controller = CanController {
            can: Can::new(&mut p.CAN1, &mut p.PA11, &mut p.PA12, Irqs),
            tx_frame: None
        };

        controller.can.modify_filters().enable_bank(0, Fifo::Fifo0, Mask32::accept_all());

        controller.can.modify_config()
            .set_loopback(true) // Receive own frames
            .set_silent(true)
            .set_bitrate(baudrate);

        controller.can.enable().await;
        controller
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