pub mod can_controller;
pub mod frame;
use crate::types::SLAVEBMS;
use crate::CanMsg;
pub use can_controller::CanController;
pub use can_controller::CanError;
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

    let mut can_second = [0u8; 4];

    can_second[0] = (bms.max_temp() & 0xFF) as u8;
    can_second[1] = ((bms.max_temp() >> 8) & 0x03) as u8 | ((bms.min_temp() & 0x3F) << 2) as u8;
    can_second[2] = ((bms.min_temp() >> 6) & 0x0F) as u8 | ((bms._avg_temp() & 0x0F) << 4) as u8;
    can_second[3] = ((bms._avg_temp() >> 4) & 0x3F) as u8 | ((0x00 & 0x01) << 6);

    let frame_send = CanFrame::new(CanMsg::TemperatureId.as_raw(), &can_second);
    match can.write(&frame_send).await {
        Ok(_) => {},

        Err(CanError::Timeout) => {
            //info!("Timeout Can connection");
            return Err(CanError::Timeout);
        }

        Err(_) => {
            //info!("Can write error");
            return Err(CanError::WriteError);
        }
    }

    let current = (bms.current() / 100) as u16;
    let current_frame = [
        get_byte!(current, 0),
        get_byte!(current, 1),
        0x00,
        0x00
    ];

    let frame_send = CanFrame::new(CanMsg::CurrentId.as_raw(), &current_frame);
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



pub async fn can_operation_tech(bms: &SLAVEBMS, can: &mut CanController<'_>) -> Result<(), CanError>{
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

    for (message, start) in [(CanMsg::Tech4, 0), (CanMsg::Tech5, 4), (CanMsg::Tech6, 8)] {
        let temperatures = [
            get_byte!(bms.temps(start), 0),
            get_byte!(bms.temps(start), 1),
            get_byte!(bms.temps(start + 1), 0),
            get_byte!(bms.temps(start + 1), 1),
            get_byte!(bms.temps(start + 2), 0),
            get_byte!(bms.temps(start + 2), 1),
            get_byte!(bms.temps(start + 3), 0),
            get_byte!(bms.temps(start + 3), 1),
        ];

        let frame_send = CanFrame::new(message.as_raw(), &temperatures);
        match can.write(&frame_send).await {
            Ok(_) => {}
            Err(CanError::Timeout) => return Err(CanError::Timeout),
            Err(_) => return Err(CanError::WriteError),
        }

        // `try_write` fills a hardware TX mailbox immediately. Leave time for
        // transmission before submitting the next temperature frame.
        embassy_time::Timer::after_millis(10).await;
    }

    Ok(())
}
