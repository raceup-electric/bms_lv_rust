//! CAN frame wrapper.
//!
//! [`CanFrame`] bundles a decoded view (id, padded data, length) together with
//! the underlying Embassy [`Frame`] ready for transmission, so the rest of the
//! firmware can work with plain `u16` ids and `[u8; 8]` payloads.

use embassy_stm32::can::{frame::Envelope, Frame, Id, StandardId};

/// A standard (11-bit id) CAN frame with an 8-byte padded payload.
#[derive(Clone)]
pub struct CanFrame {
    id: u16,
    data: [u8; 8],
    _len: usize,
    frame: Frame
}

impl CanFrame {
    /// Builds a transmit frame from an id and up to 8 data bytes.
    ///
    /// Extra bytes are ignored; shorter payloads are zero-padded in the cached
    /// `data` view (the wire frame keeps the original length).
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

    /// Builds a frame from a received Embassy envelope, normalising extended ids
    /// down to their standard part.
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

    /// Returns the underlying Embassy frame for transmission.
    pub fn frame(&self) -> Frame{
        self.frame
    }

    /// Returns the 8-byte (zero-padded) payload.
    pub fn bytes(&self) -> [u8; 8] {
        self.data
    }

    /// Returns a single payload byte.
    pub fn _byte(&self, index: usize) -> u8 {
        self.data[index]
    }

    /// Returns the standard CAN identifier.
    pub fn id(&self) -> u16 {
        self.id
    }

    /// Returns the original data length.
    pub fn _len(&self) -> usize {
        self._len
    }
}
