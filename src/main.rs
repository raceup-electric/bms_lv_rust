#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::{gpio::{Input, Pull}, Peripherals};
// use types::BMS;
use {defmt_rtt as _, panic_probe as _};

mod types;
mod can_management;
mod ltc_management;

use types::BMS;
use can_management::{CanController, CanError, CanFrame};
// use ltc_management::SpiDevice;


#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut p = embassy_stm32::init(Default::default());

    let rx_pin = Input::new(&mut p.PA11, Pull::Up);
    core::mem::forget(rx_pin);

    let mut can = setup_can(&mut p).await;
    let mut bms = setup_bms().await;

    

    let mut i: u8 = 0;
    
    loop {

        let frame = CanFrame::new(25, &[i]);

        match can.write(&frame).await {
            Ok(_) => {
                info!("Message sent! {}", &frame.id());
            }

            Err(CanError::Timeout) => {
                info!("Timeout Can connection");
            }

            Err(_) => {
                info!("Can write error");
            }
        }

        let ex_frame = can.read().await;

        match ex_frame {
            Ok(_) => {
                info!("Out: {}", ex_frame.unwrap().byte(0));
            }

            Err(_) => {
                info!("No messages");
            }
        }

        i = i.wrapping_add(1);
    }
}

async fn setup_can(p: &mut Peripherals) -> CanController<'_>{
    let can = CanController::new(p, 500_000).await;
    can
}

async fn setup_bms() -> BMS{
    let bms = BMS::new().await;
    bms
}