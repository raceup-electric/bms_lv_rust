//! bxCAN controller wrapper.
//!
//! Wraps the Embassy [`Can`] peripheral and binds the required interrupt
//! handlers for both CAN1 and CAN2. On the STM32F4 the two bxCAN instances
//! share a single filter bank: CAN2 is the "slave" instance physically wired to
//! the bus, but CAN1 (the "master") must still be configured so the filters
//! work. [`CanController::new_can2`] sets this up, then puts CAN1 to sleep.

pub use super::CanFrame;

use embassy_stm32::bind_interrupts;
use embassy_stm32::can::filter::Mask32;
use embassy_stm32::can::{
    Can, Fifo, Rx0InterruptHandler, Rx1InterruptHandler, SceInterruptHandler, TxInterruptHandler
};

use embassy_stm32::peripherals::{CAN1, CAN2, PA11, PA12, PB12, PB13};
use embassy_time::Duration;

// Interrupt handlers for the CAN1 (master) instance.
bind_interrupts!(struct Irqs1 {
    CAN1_RX0 => Rx0InterruptHandler<CAN1>;
    CAN1_RX1 => Rx1InterruptHandler<CAN1>;
    CAN1_SCE => SceInterruptHandler<CAN1>;
    CAN1_TX => TxInterruptHandler<CAN1>;
});

// Interrupt handlers for the CAN2 (slave) instance used on the bus.
bind_interrupts!(struct Irqs2 {
    CAN2_RX0 => Rx0InterruptHandler<CAN2>;
    CAN2_RX1 => Rx1InterruptHandler<CAN2>;
    CAN2_SCE => SceInterruptHandler<CAN2>;
    CAN2_TX => TxInterruptHandler<CAN2>;
});


/// Errors returned by the CAN controller.
#[derive(Debug)]
pub enum CanError {
    /// Nothing available to read.
    NoItem,
    /// A previous frame was still pending after several retries.
    Timeout,
    /// The hardware mailbox rejected the frame.
    WriteError,
}

/// Owns the active CAN peripheral and a single pending TX frame slot.
pub struct CanController<'a> {
    can: Can<'a>,
    tx_frame: Option<CanFrame>,
    is_can2: bool
}


impl<'a> CanController<'a>{
    /// Applies the common runtime configuration (bitrate, no loopback/silent,
    /// no auto-retransmit) and enables the peripheral. The master instance also
    /// installs an accept-all filter.
    async fn new(mut controller: CanController<'a>, baudrate: u32) -> Self {
        controller.can.modify_config()
            .set_loopback(false)             // Disable loopback mode
            .set_silent(false)               // Enable active participation in the bus
            .set_automatic_retransmit(false) // Disable automatic retransmission
            .set_bitrate(baudrate);

        if !controller.is_can2 {controller.can.modify_filters().enable_bank(0, Fifo::Fifo0, Mask32::accept_all());}
        controller.can.enable().await;
        controller
    }

    /// Builds a controller on the CAN1 instance (currently unused; CAN2 is the
    /// bus interface).
    pub async fn _new_can1(peri: CAN1, rx: PA11, tx: PA12, baudrate: u32) -> Self {
        let controller = CanController {
            can: Can::new(
                peri,
                rx,
                tx,
                Irqs1
            ),
            tx_frame: None,
            is_can2: false
        };
        Self::new(controller, baudrate).await
    }

    /// Builds a controller on the CAN2 instance.
    ///
    /// CAN1 is briefly initialised to split the shared filter bank and route a
    /// slave accept-all filter to FIFO1, then put to sleep. Its pins are
    /// returned so the caller can reuse them (here, for USB).
    pub async fn new_can2(peri: CAN2, rx: PB12, tx: PB13, baudrate: u32, peri1: CAN1, mut rx1: PA11, mut tx1: PA12) -> (Self, PA11, PA12) {
        let mut can1 = Can::new(peri1, &mut rx1, &mut tx1, Irqs1);

        let controller = CanController {
            can: Can::new(peri, rx, tx, Irqs2),
            tx_frame: None,
            is_can2: true
        };

        // Give the whole filter bank to the slave (CAN2) and accept everything.
        can1.modify_filters().set_split(0).num_banks();
        can1.modify_filters().slave_filters().enable_bank(0, Fifo::Fifo1, Mask32::accept_all());
        can1.sleep().await;
        drop(can1);

        (Self::new(controller, baudrate).await, rx1, tx1)
    }

    /// Transmits a frame, retrying on a busy mailbox.
    ///
    /// Waits (up to ~50 ms) for any previous pending frame to clear, then tries
    /// the hardware write a few times. Returns [`CanError::Timeout`] if the slot
    /// never freed, or [`CanError::WriteError`] if every write attempt failed.
    pub async fn write(&mut self, frame: &CanFrame) -> Result<(), CanError> {
        let mut attempts: u8 = 0;

        // Wait for any in-flight frame to be consumed.
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

        // Try to push the frame into a hardware mailbox.
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

    /// Non-blocking read of one frame; returns [`CanError::NoItem`] if the RX
    /// FIFO is empty.
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
