pub mod frame;
pub mod can_controller;
pub use can_controller::CanError;
use crate::CanMsg;
use crate::BMS;
use defmt::info;
pub use frame::CanFrame;
pub use can_controller::CanController;


#[macro_export]
macro_rules! get_byte {
    ($value:expr, $byte_num:expr) => {
        (($value >> ($byte_num * 8)) & 0xFF) as u8
    };
    
    ($array:expr, $byte_num:expr, slice) => {
        $array.get($byte_num).copied().unwrap_or(0)
    };
}

pub async fn can_operation(bms: &BMS, can: &mut CanController<'_>) {
    static mut TEMP: usize = 0;
    unsafe {
        let can_first: [u8; 8] = [
            get_byte!(bms.cell_volts[TEMP], 0),
            get_byte!(bms.cell_volts[TEMP], 1),
            0, 0,0,0,0,0
        ];
        TEMP.wrapping_add(1);
        TEMP = TEMP %12;
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
        }

        Err(_) => {
            info!("Can write error");
        }
    }

}
    let can_second = [
        get_byte!(bms.temp(), 0),
        get_byte!(bms.temp(), 1),
        0,
        0,
        0,
        0,
        0,
        0
    ];

    let frame_send = CanFrame::new(CanMsg::TemperatureId.as_raw(), &can_second);
    match can.write(&frame_send).await {
        Ok(_) => {
            info!("Message sent! {}", &frame_send.id());
            for i in 0..frame_send.len() {
                info!("Byte: {}: {}", i, &frame_send.byte(i));
            }
        }

        Err(CanError::Timeout) => {
            info!("Timeout Can connection");
        }

        Err(_) => {
            info!("Can write error");
        }
    }
}