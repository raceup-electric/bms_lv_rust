pub mod can_controller;
pub mod frame;
use crate::types::soc::{remaining_energy_deci_wh, soc_percentage};
use crate::types::SLAVEBMS;
use crate::CanMsg;
pub use can_controller::CanController;
pub use can_controller::CanError;
pub use can_controller::CanReceiver;
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

pub async fn can_operation(
    bms: &SLAVEBMS,
    can: &mut CanController<'_>,
    fault_temp: bool,
) -> Result<(), CanError> {
    let tot_v = (bms.tot_volt() / 100) as u16;

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

    let frame_send = CanFrame::new(CanMsg::VoltageId.as_raw(), &can_first);

    match can.write(&frame_send).await {
        Ok(_) => (),
        Err(CanError::Timeout) => return Err(CanError::Timeout),
        Err(_) => return Err(CanError::WriteError),
    }

    let mut can_second = [0u8; 4];

    can_second[0] = (bms.max_temp() & 0xFF) as u8;
    can_second[1] = ((bms.max_temp() >> 8) & 0x03) as u8 | ((bms.min_temp() & 0x3F) << 2) as u8;
    can_second[2] = ((bms.min_temp() >> 6) & 0x0F) as u8 | ((bms._avg_temp() & 0x0F) << 4) as u8;
    can_second[3] = ((bms._avg_temp() >> 4) & 0x3F) as u8;
    can_second[3] |= u8::from(fault_temp) << 6;

    let frame_send = CanFrame::new(CanMsg::TemperatureId.as_raw(), &can_second);

    match can.write(&frame_send).await {
        Ok(_) => (),
        Err(CanError::Timeout) => return Err(CanError::Timeout),
        Err(_) => return Err(CanError::WriteError),
    }

    let current = (bms.current() / 100) as u16;
    let (remaining_energy, soc) = if bms.cell_data_valid() {
        (
            remaining_energy_deci_wh(bms.tot_volt()),
            soc_percentage(bms.tot_volt()),
        )
    } else {
        (0, 0)
    };
    let current_frame = [
        get_byte!(current, 0),
        get_byte!(current, 1),
        get_byte!(remaining_energy, 0),
        get_byte!(remaining_energy, 1),
        soc,
    ];

    let frame_send = CanFrame::new(CanMsg::CurrentId.as_raw(), &current_frame);

    match can.write(&frame_send).await {
        Ok(_) => (),
        Err(CanError::Timeout) => return Err(CanError::Timeout),
        Err(_) => return Err(CanError::WriteError),
    }

    Ok(())
}

pub async fn can_operation_tech(
    bms: &SLAVEBMS,
    can: &mut CanController<'_>,
) -> Result<(), CanError> {
    let can_first: [u8; 8] = [
        get_byte!(bms.cell_volts(0), 0),
        get_byte!(bms.cell_volts(0), 1),
        get_byte!(bms.cell_volts(1), 0),
        get_byte!(bms.cell_volts(1), 1),
        get_byte!(bms.cell_volts(2), 0),
        get_byte!(bms.cell_volts(2), 1),
        get_byte!(bms.cell_volts(3), 0),
        get_byte!(bms.cell_volts(3), 1),
    ];

    let frame_send = CanFrame::new(CanMsg::Tech1.as_raw(), &can_first);
    match can.write(&frame_send).await {
        Ok(_) => (),
        Err(CanError::Timeout) => return Err(CanError::Timeout),
        Err(_) => return Err(CanError::WriteError),
    }

    let can_second = [
        get_byte!(bms.cell_volts(4), 0),
        get_byte!(bms.cell_volts(4), 1),
        get_byte!(bms.cell_volts(5), 0),
        get_byte!(bms.cell_volts(5), 1),
        get_byte!(bms.cell_volts(6), 0),
        get_byte!(bms.cell_volts(6), 1),
        get_byte!(bms.cell_volts(7), 0),
        get_byte!(bms.cell_volts(7), 1),
    ];

    let frame_send = CanFrame::new(CanMsg::Tech2.as_raw(), &can_second);
    match can.write(&frame_send).await {
        Ok(_) => (),
        Err(CanError::Timeout) => return Err(CanError::Timeout),
        Err(_) => return Err(CanError::WriteError),
    }

    let can_third = [
        get_byte!(bms.cell_volts(8), 0),
        get_byte!(bms.cell_volts(8), 1),
        get_byte!(bms.cell_volts(9), 0),
        get_byte!(bms.cell_volts(9), 1),
        get_byte!(bms.cell_volts(10), 0),
        get_byte!(bms.cell_volts(10), 1),
        get_byte!(bms.cell_volts(11), 0),
        get_byte!(bms.cell_volts(11), 1),
    ];

    embassy_time::Timer::after_millis(10).await;

    let frame_send = CanFrame::new(CanMsg::Tech3.as_raw(), &can_third);
    match can.write(&frame_send).await {
        Ok(_) => (),
        Err(CanError::Timeout) => return Err(CanError::Timeout),
        Err(_) => return Err(CanError::WriteError),
    }

    let mut temps_frame1 = [0u8; 8];
    for i in 0..4 {
        let t = bms.temps(i);
        temps_frame1[i * 2] = get_byte!(t, 0);
        temps_frame1[i * 2 + 1] = get_byte!(t, 1);
    }
    let frame_send = CanFrame::new(CanMsg::BMSLVTemps1.as_raw(), &temps_frame1);
    match can.write(&frame_send).await {
        Ok(_) => (),
        Err(CanError::Timeout) => return Err(CanError::Timeout),
        Err(_) => return Err(CanError::WriteError),
    }

    let mut temps_frame2 = [0u8; 8];
    for i in 0..4 {
        let idx = 4 + i;
        let t = bms.temps(idx);
        temps_frame2[i * 2] = get_byte!(t, 0);
        temps_frame2[i * 2 + 1] = get_byte!(t, 1);
    }
    let frame_send = CanFrame::new(CanMsg::BMSLVTemps2.as_raw(), &temps_frame2);
    match can.write(&frame_send).await {
        Ok(_) => (),
        Err(CanError::Timeout) => return Err(CanError::Timeout),
        Err(_) => return Err(CanError::WriteError),
    }

    let mut temps_frame3 = [0u8; 8];
    for i in 0..4 {
        let idx = 8 + i;
        let t = bms.temps(idx);
        temps_frame3[i * 2] = get_byte!(t, 0);
        temps_frame3[i * 2 + 1] = get_byte!(t, 1);
    }
    let frame_send = CanFrame::new(CanMsg::BMSLVTemps3.as_raw(), &temps_frame3);
    match can.write(&frame_send).await {
        Ok(_) => (),
        Err(CanError::Timeout) => return Err(CanError::Timeout),
        Err(_) => return Err(CanError::WriteError),
    }

    Ok(())
}
