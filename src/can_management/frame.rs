use embassy_stm32::can::frame::{Envelope, Frame};
use embedded_can::Id;

#[derive(Clone)]
pub struct CanFrame {
    id: u16,
    data: [u8; 8],
    frame: Frame,
}

impl CanFrame {
    pub fn new(id: u16, data: &[u8]) -> Self {
        let mut frame_data = [0u8; 8];
        let len = data.len().min(8);

        frame_data[..len].copy_from_slice(&data[..len]);

        let tx_frame = Frame::new_standard(id, &frame_data[..len]).unwrap();

        Self {
            id,
            data: frame_data,
            frame: tx_frame,
        }
    }

    pub fn from_envelope(envelope: Envelope) -> Self {
        let (rx_frame, _) = envelope.parts();
        let mut frame_data = [0u8; 8];
        let len: usize = rx_frame.header().len().min(8) as usize;

        frame_data[..len].copy_from_slice(&rx_frame.data()[..len]);

        let id = match *rx_frame.id() {
            Id::Standard(id) => id.as_raw(),
            Id::Extended(id) => id.as_raw() as u16,
        };

        Self {
            id,
            data: frame_data,
            frame: rx_frame,
        }
    }

    pub fn frame(&self) -> Frame {
        self.frame
    }

    pub fn bytes(&self) -> [u8; 8] {
        self.data
    }

    pub fn id(&self) -> u16 {
        self.id
    }
}
