//! CAN communication layer.
//!
//! Re-exports the [`CanController`], [`CanFrame`] and [`CanError`] types and
//! provides the helper routines that pack the battery state into CAN frames:
//!
//! * [`can_operation`] sends the two summary frames (voltages, then
//!   temperatures + current).
//! * [`can_operation_tech`] sends four extended frames carrying every cell
//!   voltage and every thermistor temperature.

pub mod can_controller;
pub mod frame;
use crate::types::SLAVEBMS;
use crate::CanMsg;
pub use can_controller::CanController;
pub use can_controller::CanError;
pub use frame::CanFrame;

/// Extracts a single byte from an integer, or from a slice by index.
///
/// * `get_byte!(value, n)` returns byte `n` (little-endian) of `value`.
/// * `get_byte!(array, n, slice)` returns `array[n]`, or `0` if out of range.
#[macro_export]
macro_rules! get_byte {
    ($value:expr, $byte_num:expr) => {
        (($value >> ($byte_num * 8)) & 0xFF) as u8
    };

    ($array:expr, $byte_num:expr, slice) => {
        $array.get($byte_num).copied().unwrap_or(0)
    };
}

/// Sends the two summary frames describing the pack state.
///
/// Frame [`CanMsg::VoltageId`] carries max/min/avg cell voltage and the total
/// pack voltage (scaled by 1/100). Frame [`CanMsg::TemperatureId`] carries
/// max/min temperature and the 32-bit pack current.
pub async fn can_operation(bms: &SLAVEBMS, can: &mut CanController<'_>) -> Result<(), CanError>{
    let tot_v = (bms.tot_volt()/100) as u16;
    static mut TEMP: usize = 0 as usize;
    unsafe {
        // Pack voltage summary into 8 bytes (little-endian u16 fields).
        let can_first: [u8; 8] = [
            get_byte!(bms.max_volt(), 0),
            get_byte!(bms.max_volt(), 1),
            get_byte!(bms.min_volt(), 0),
            get_byte!(bms.min_volt(), 1),
            get_byte!(bms.avg_volt(), 0),
            get_byte!(bms.avg_volt(), 1),
            get_byte!(tot_v, 0),
            get_byte!(tot_v, 1),
        ];
        TEMP = TEMP.wrapping_add(1);
        if TEMP == (12 as usize) {
            TEMP = 0 as usize;
        }
        let frame_send = CanFrame::new(CanMsg::VoltageId.as_raw(), &can_first);
        match can.write(&frame_send).await {
            Ok(_) => {}

            Err(CanError::Timeout) => {
                //info!("Timeout Can connection");
                return Err(CanError::Timeout);
            }

            Err(_) => {
                //info!("Can write error");
                return Err(CanError::WriteError);
            }
        }
    }

    // Temperature summary + 32-bit current.
    let can_second = [
        get_byte!(bms.max_temp(), 0),
        get_byte!(bms.max_temp(), 1),
        get_byte!(bms.min_temp(), 0),
        get_byte!(bms.min_temp(), 1),
        get_byte!(bms.current(), 0),
        get_byte!(bms.current(), 1),
        get_byte!(bms.current(), 2),
        get_byte!(bms.current(), 3)
    ];

    let frame_send = CanFrame::new(CanMsg::TemperatureId.as_raw(), &can_second);
    match can.write(&frame_send).await {
        Ok(_) => Ok(()),

        Err(CanError::Timeout) => {
            //info!("Timeout Can connection");
            return Err(CanError::Timeout);
        }

        Err(_) => {
            //info!("Can write error");
            return Err(CanError::WriteError);
        }
    }
}



/// Sends the extended per-cell telemetry (four frames).
///
/// `Tech1`/`Tech2`/`Tech3` carry cells 1-4, 5-8 and 9-12 respectively; `Tech4`
/// carries the four thermistor temperatures. Each value is a little-endian
/// `u16`.
pub async fn can_operation_tech(bms: &SLAVEBMS, can: &mut CanController<'_>) -> Result<(), CanError>{
    // Cells 1-4.
    let can_first: [u8; 8] = [
        get_byte!(bms.cell_volts(0), 0),
        get_byte!(bms.cell_volts(0), 1),
        get_byte!(bms.cell_volts(1), 0),
        get_byte!(bms.cell_volts(1), 1),
        get_byte!(bms.cell_volts(2), 0),
        get_byte!(bms.cell_volts(2), 1),
        get_byte!(bms.cell_volts(3), 0),
        get_byte!(bms.cell_volts(3), 1)
    ];
    let frame_send = CanFrame::new(CanMsg::Tech1.as_raw(), &can_first);
    match can.write(&frame_send).await {
        Ok(_) => {}

        Err(CanError::Timeout) => {
            //info!("Timeout Can connection");
            return Err(CanError::Timeout);
        }

        Err(_) => {
            //info!("Can write tech error");
            return Err(CanError::WriteError);
        }
    }

    // Cells 5-8.
    let can_second = [
        get_byte!(bms.cell_volts(4), 0),
        get_byte!(bms.cell_volts(4), 1),
        get_byte!(bms.cell_volts(5), 0),
        get_byte!(bms.cell_volts(5), 1),
        get_byte!(bms.cell_volts(6), 0),
        get_byte!(bms.cell_volts(6), 1),
        get_byte!(bms.cell_volts(7), 0),
        get_byte!(bms.cell_volts(7), 1)
    ];

    let frame_send = CanFrame::new(CanMsg::Tech2.as_raw(), &can_second);
    match can.write(&frame_send).await {
        Ok(_) => {}


        Err(CanError::Timeout) => {
            //info!("Timeout Can connection");
            return Err(CanError::Timeout);
        }

        Err(_) => {
            //info!("Can tech write error");
            return Err(CanError::WriteError);
        }
    }

    // Cells 9-12.
    let can_third = [
        get_byte!(bms.cell_volts(8), 0),
        get_byte!(bms.cell_volts(8), 1),
        get_byte!(bms.cell_volts(9), 0),
        get_byte!(bms.cell_volts(9), 1),
        get_byte!(bms.cell_volts(10), 0),
        get_byte!(bms.cell_volts(10), 1),
        get_byte!(bms.cell_volts(11), 0),
        get_byte!(bms.cell_volts(11), 1)
    ];

    embassy_time::Timer::after_millis(10).await;

    let frame_send = CanFrame::new(CanMsg::Tech3.as_raw(), &can_third);
    match can.write(&frame_send).await {
        Ok(_) => {}

        Err(CanError::Timeout) => {
            //info!("Timeout Can tech connection");
            return Err(CanError::Timeout);
        }

        Err(_) => {
            //info!("Can tech write error");
            return Err(CanError::WriteError);
        }
    }

    // Thermistor temperatures.
    let can_fourth = [
        get_byte!(bms.temps(0), 0),
        get_byte!(bms.temps(0), 1),
        get_byte!(bms.temps(1), 0),
        get_byte!(bms.temps(1), 1),
        get_byte!(bms.temps(2), 0),
        get_byte!(bms.temps(2), 1),
        get_byte!(bms.temps(3), 0),
        get_byte!(bms.temps(3), 1),
    ];

    let frame_send = CanFrame::new(CanMsg::Tech4.as_raw(), &can_fourth);
    match can.write(&frame_send).await {
        Ok(_) => {
            Ok(())
        }

        Err(CanError::Timeout) => {
            //info!("Timeout Can tech connection");
            return Err(CanError::Timeout);
        }

        Err(_) => {
            //info!("Can tech write error");
            return Err(CanError::WriteError);
        }
    }
}
