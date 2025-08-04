use embassy_stm32::can::{frame::Envelope, Frame, Id, StandardId};

#[derive(Clone)]
pub struct CanFrame {
    id: u16,
    data: [u8; 8],
    _len: usize,
    frame: Frame
}

impl CanFrame {
    pub fn new(id: u16, data: &[u8]) -> Self {
        let mut frame_data = [0u8; 8]; 
        let _len = data.len().min(8);

        frame_data[.._len].copy_from_slice(&data[.._len]);

        let tx_frame = Frame::new_data(
            StandardId::new(id as _).unwrap(),
            data,
        ).unwrap();

        CanFrame {
            id,
            data: frame_data,
            _len,
            frame: tx_frame
        }
    }

    pub fn from_envelope(envelope: Envelope) -> Self {
        let rx_frame = envelope.frame;
        let mut frame_data = [0u8; 8]; 
        let len: usize = rx_frame.header().len().min(8) as usize;

        frame_data[..len].copy_from_slice(&rx_frame.data()[..len]);

        let id = match rx_frame.id() {
            Id::Standard(id) => id.as_raw(), 
            Id::Extended(id) => id.standard_id().as_raw(), 
        };

        CanFrame {
            id,
            data: frame_data,
            _len: rx_frame.header().len() as usize,
            frame: rx_frame
        }
    }

    pub fn frame(&self) -> Frame{
        self.frame
    }

    pub fn bytes(&self) -> [u8; 8] {
        self.data
    }

    pub fn _byte(&self, index: usize) -> u8 {
        self.data[index]
    }

    pub fn id(&self) -> u16 {
        self.id
    }

    pub fn _len(&self) -> usize {
        self._len
    }
}