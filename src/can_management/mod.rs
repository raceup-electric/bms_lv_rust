pub mod can_controller;
pub mod frame;
use crate::types::SLAVEBMS;
use crate::CanMsg;
pub use can_controller::CanController;
pub use can_controller::CanError;
use defmt::info;
pub use frame::CanFrame;

#[macro_export]
macro_rules! get_byte {
    ($value:expr, $byte_num:expr) => {
        (($value >> ($byte_num * 8)) & 0xFF) as u8
    };

    ($array:expr, $byte_num:expr, slice) => {
        $array.get($byte_num).copied().unwrap_or(0)
    };
}

pub async fn can_operation(bms: &SLAVEBMS, can: &mut CanController<'_>) -> Result<(), CanError>{
    let tot_v = (bms.tot_volt()/100) as u16;
    static mut TEMP: usize = 0 as usize;
    unsafe {
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
            Ok(_) => {
                info!("Message sent! {}", &frame_send.id());
                for i in 0..frame_send.len() {
                    info!("Byte: {}: {}", i, &frame_send.byte(i));
                }
            }

            Err(CanError::Timeout) => {
                info!("Timeout Can connection");
                return Err(CanError::Timeout);
            }

            Err(_) => {
                info!("Can write error");
                return Err(CanError::WriteError);
            }
        }
    }

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
        Ok(_) => {
            info!("Message sent! {}", &frame_send.id());
            for i in 0..frame_send.len() {
                info!("Byte: {}: {}", i, &frame_send.byte(i));
            }
            Ok(())

        }

        Err(CanError::Timeout) => {
            info!("Timeout Can connection");
            return Err(CanError::Timeout);
        }

        Err(_) => {
            info!("Can write error");
            return Err(CanError::WriteError);
        }
    }
}