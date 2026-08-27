use super::spi_device::SpiDevice;
use crate::types::{bms::SLAVEBMS, VOLTAGES};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_time::{Duration, Timer};

use libm::{logf, roundf};

pub const WRCFGA: [u8; 2] = [0x00, 0x01];
pub const RDCFGA: [u8; 2] = [0x00, 0x02];
pub const RDCVA: [u8; 2] = [0x00, 0x04];
pub const RDCVB: [u8; 2] = [0x00, 0x06];
pub const RDCVC: [u8; 2] = [0x00, 0x08];
pub const RDCVD: [u8; 2] = [0x00, 0x0A];
pub const RDAUXA: [u8; 2] = [0x00, 0x0C];
pub const RDAUXB: [u8; 2] = [0x00, 0x0E];
pub const ADCV: [u8; 2] = [0x02, 0x60];
pub const ADAX: [u8; 2] = [0x04, 0x80];
pub const PLADC: [u8; 2] = [0x07, 0x14];

pub const WRCOMM: [u8; 2] = [0x07, 0x21];
pub const STCOMM: [u8; 2] = [0x07, 0x23];

const THERMISTOR_PULLUP_OHM: u32 = 10_000;
const R25: f32 = 9.914;
const B_COEFF: f32 = 3435.0;
const KELVIN_2_CELSIUS: f32 = 273.15;
const MAX_TEMP: u16 = u16::MAX;
const MIN_TEMP: u16 = 0;
const BAL_EPSILON: i16 = 50;
const NUM_CELLS: usize = 12;
const REFON: u8 = 0x01 << 2;
const ADCOPT: u8 = 0x00;
const GPIO1: u8 = 0x01;
const GPIO2: u8 = 0x01;
const GPIO3: u8 = 0x01;
const GPIO4: u8 = 0x01;
const GPIO5: u8 = 0x01;
const GPIOS: u8 = (GPIO1 << 3) | (GPIO2 << 4) | (GPIO3 << 5) | (GPIO4 << 6) | (GPIO5 << 7);

const CRC15_TABLE: [u16; 256] = [
    0x0, 0xc599, 0xceab, 0xb32, 0xd8cf, 0x1d56, 0x1664, 0xd3fd, 0xf407, 0x319e, 0x3aac, 0xff35,
    0x2cc8, 0xe951, 0xe263, 0x27fa, 0xad97, 0x680e, 0x633c, 0xa6a5, 0x7558, 0xb0c1, 0xbbf3, 0x7e6a,
    0x5990, 0x9c09, 0x973b, 0x52a2, 0x815f, 0x44c6, 0x4ff4, 0x8a6d, 0x5b2e, 0x9eb7, 0x9585, 0x501c,
    0x83e1, 0x4678, 0x4d4a, 0x88d3, 0xaf29, 0x6ab0, 0x6182, 0xa41b, 0x77e6, 0xb27f, 0xb94d, 0x7cd4,
    0xf6b9, 0x3320, 0x3812, 0xfd8b, 0x2e76, 0xebef, 0xe0dd, 0x2544, 0x2be, 0xc727, 0xcc15, 0x98c,
    0xda71, 0x1fe8, 0x14da, 0xd143, 0xf3c5, 0x365c, 0x3d6e, 0xf8f7, 0x2b0a, 0xee93, 0xe5a1, 0x2038,
    0x7c2, 0xc25b, 0xc969, 0xcf0, 0xdf0d, 0x1a94, 0x11a6, 0xd43f, 0x5e52, 0x9bcb, 0x90f9, 0x5560,
    0x869d, 0x4304, 0x4836, 0x8daf, 0xaa55, 0x6fcc, 0x64fe, 0xa167, 0x729a, 0xb703, 0xbc31, 0x79a8,
    0xa8eb, 0x6d72, 0x6640, 0xa3d9, 0x7024, 0xb5bd, 0xbe8f, 0x7b16, 0x5cec, 0x9975, 0x9247, 0x57de,
    0x8423, 0x41ba, 0x4a88, 0x8f11, 0x57c, 0xc0e5, 0xcbd7, 0xe4e, 0xddb3, 0x182a, 0x1318, 0xd681,
    0xf17b, 0x34e2, 0x3fd0, 0xfa49, 0x29b4, 0xec2d, 0xe71f, 0x2286, 0xa213, 0x678a, 0x6cb8, 0xa921,
    0x7adc, 0xbf45, 0xb477, 0x71ee, 0x5614, 0x938d, 0x98bf, 0x5d26, 0x8edb, 0x4b42, 0x4070, 0x85e9,
    0xf84, 0xca1d, 0xc12f, 0x4b6, 0xd74b, 0x12d2, 0x19e0, 0xdc79, 0xfb83, 0x3e1a, 0x3528, 0xf0b1,
    0x234c, 0xe6d5, 0xede7, 0x287e, 0xf93d, 0x3ca4, 0x3796, 0xf20f, 0x21f2, 0xe46b, 0xef59, 0x2ac0,
    0xd3a, 0xc8a3, 0xc391, 0x608, 0xd5f5, 0x106c, 0x1b5e, 0xdec7, 0x54aa, 0x9133, 0x9a01, 0x5f98,
    0x8c65, 0x49fc, 0x42ce, 0x8757, 0xa0ad, 0x6534, 0x6e06, 0xab9f, 0x7862, 0xbdfb, 0xb6c9, 0x7350,
    0x51d6, 0x944f, 0x9f7d, 0x5ae4, 0x8919, 0x4c80, 0x47b2, 0x822b, 0xa5d1, 0x6048, 0x6b7a, 0xaee3,
    0x7d1e, 0xb887, 0xb3b5, 0x762c, 0xfc41, 0x39d8, 0x32ea, 0xf773, 0x248e, 0xe117, 0xea25, 0x2fbc,
    0x846, 0xcddf, 0xc6ed, 0x374, 0xd089, 0x1510, 0x1e22, 0xdbbb, 0xaf8, 0xcf61, 0xc453, 0x1ca,
    0xd237, 0x17ae, 0x1c9c, 0xd905, 0xfeff, 0x3b66, 0x3054, 0xf5cd, 0x2630, 0xe3a9, 0xe89b, 0x2d02,
    0xa76f, 0x62f6, 0x69c4, 0xac5d, 0x7fa0, 0xba39, 0xb10b, 0x7492, 0x5368, 0x96f1, 0x9dc3, 0x585a,
    0x8ba7, 0x4e3e, 0x450c, 0x8095,
];

#[derive(Debug, PartialEq, Clone)]
pub enum MODE {
    NORMAL,
    BALANCING,
}

pub struct LTC6811 {
    spi: &'static Mutex<CriticalSectionRawMutex, SpiDevice<'static>>,
    bms: &'static Mutex<CriticalSectionRawMutex, SLAVEBMS>,
    config: [u8; 6],
    mode: MODE,
    prev_mode: MODE,
}

const NUM_THERMISTORS: usize = 12;
const MUXES: usize = 2;
const CHANNELS_PER_MUX: usize = 8;

impl LTC6811 {
    pub async fn new(
        spi: &'static Mutex<CriticalSectionRawMutex, SpiDevice<'static>>,
        bms: &'static Mutex<CriticalSectionRawMutex, SLAVEBMS>,
    ) -> Self {
        let config = [GPIOS | ADCOPT | REFON, 0x00, 0x00, 0x00, 0x00, 0x00];

        LTC6811 {
            spi,
            bms,
            config,
            mode: MODE::NORMAL,
            prev_mode: MODE::NORMAL,
        }
    }

    pub fn calculate_pec(&self, data: &[u8]) -> [u8; 2] {
        let mut remainder: u16 = 16;
        for byte in data {
            let address: usize = (((remainder >> 7) ^ (*byte as u16)) & 0xff).into();
            remainder = (remainder << 8) ^ CRC15_TABLE[address];
        }
        remainder <<= 1;
        [(remainder >> 8) as u8, remainder as u8]
    }

    pub async fn set_mode(&mut self, mode: MODE) {
        self.mode = mode.clone();
        if self.prev_mode != mode || mode == MODE::BALANCING {
            let _ = self.init_cfg().await;
            self.prev_mode = mode;
        }
    }

    fn prepare_command(&self, cmd: [u8; 2]) -> [u8; 4] {
        let mut cmd_f = [0u8; 4];
        cmd_f[0..2].copy_from_slice(&cmd);
        cmd_f[2..4].copy_from_slice(&self.calculate_pec(&cmd));
        cmd_f
    }

    pub async fn init_cfg(&mut self) -> Result<(), ()> {
        let uv_val = (VOLTAGES::MINVOLTAGE.as_raw() / 16) - 1;
        let ov_val = VOLTAGES::MAXVOLTAGE.as_raw() / 16;

        self.config[0] = GPIOS | ADCOPT | REFON;
        self.config[1] = (uv_val & 0xFF) as u8;
        self.config[2] = (((ov_val & 0xF) << 4) | ((uv_val & 0xF00) >> 8)) as u8;
        self.config[3] = (ov_val >> 4) as u8;

        {
            let bms_data = self.bms.lock().await;
            if self.mode == MODE::BALANCING && bms_data.min_volt() != 0 && bms_data.max_volt() != 0
            {
                let mut discharge_bitmap: u16 = 0;
                for i in 0..NUM_CELLS {
                    if (bms_data.cell_volts(i) as i16 - bms_data.min_volt() as i16) > BAL_EPSILON {
                        discharge_bitmap |= 1 << i;
                    }
                }
                self.config[4] = (discharge_bitmap & 0xFF) as u8;
                self.config[5] = ((discharge_bitmap >> 8) & 0x0F) as u8;
            } else {
                self.config[4] = 0x00;
                self.config[5] = 0x00;
            }
            drop(bms_data);
        }

        self.write_config().await?;
        Ok(())
    }

    pub async fn init(&mut self) -> Result<(), ()> {
        self.init_cfg().await?;
        self.wakeup().await;
        Timer::after(Duration::from_millis(10)).await;

        let mut read_config = [0u8; 8];
        let cmd = self.prepare_command(RDCFGA);
        let mut spi_data = self.spi.lock().await;
        spi_data.cmd_read(&cmd, &mut read_config).await.unwrap();
        drop(spi_data);
        let _config_valid = read_config[..6] == self.config
            && read_config[6..8] == self.calculate_pec(&read_config[..6]);

        Ok(())
    }

    pub async fn wakeup(&mut self) {
        let mut spi_data = self.spi.lock().await;
        spi_data.cs.set_low();
        for _ in 0..50 {
            spi_data.write(&[0xff]).await;
        }
        spi_data.cs.set_high();
        drop(spi_data);
    }

    pub async fn wakeup_idle(&mut self) {
        let mut spi_data = self.spi.lock().await;
        spi_data.cs.set_low();
        spi_data.write(&[0xFF; 8]).await;
        spi_data.cs.set_high();
        drop(spi_data);
    }

    pub async fn write_config(&mut self) -> Result<(), ()> {
        let cmd = self.prepare_command(WRCFGA);
        let mut data = [0u8; 8];
        data[0..6].copy_from_slice(&self.config);
        let pec = self.calculate_pec(&self.config);
        data[6] = pec[0];
        data[7] = pec[1];

        let mut cmd_final = [0u8; 12];
        cmd_final[0..4].copy_from_slice(&cmd);
        cmd_final[4..12].copy_from_slice(&data);

        self.wakeup_idle().await;
        let mut spi_data = self.spi.lock().await;
        spi_data.write(&cmd_final).await;
        drop(spi_data);
        Ok(())
    }

    pub async fn start_cell_conversion(&mut self) -> Result<(), ()> {
        let cmd = self.prepare_command(ADCV);
        self.wakeup_idle().await;
        let mut spi_data = self.spi.lock().await;
        spi_data.write(&cmd).await;
        drop(spi_data);

        let poll = self.prepare_command(PLADC);
        loop {
            let mut spi_data = self.spi.lock().await;
            let mut status = [0u8; 8];
            spi_data.cmd_read(&poll, &mut status).await.unwrap();
            if status[0] & 0x01 != 0 {
                break;
            }
            drop(spi_data);
            embassy_time::Timer::after_millis(1).await;
        }
        Ok(())
    }

    pub async fn read_cell_voltages(&mut self) -> Result<(), ()> {
        self.start_cell_conversion().await?;
        self.wakeup_idle().await;
        let mut spi_data = self.spi.lock().await;

        let cmd_a = self.prepare_command(RDCVA);
        let mut data_a = [0u8; 8];
        spi_data.cmd_read(&cmd_a, &mut data_a).await.unwrap();

        let cmd_b = self.prepare_command(RDCVB);
        let mut data_b = [0u8; 8];
        spi_data.cmd_read(&cmd_b, &mut data_b).await.unwrap();

        let cmd_c = self.prepare_command(RDCVC);
        let mut data_c = [0u8; 8];
        spi_data.cmd_read(&cmd_c, &mut data_c).await.unwrap();

        let cmd_d = self.prepare_command(RDCVD);
        let mut data_d = [0u8; 8];
        spi_data.cmd_read(&cmd_d, &mut data_d).await.unwrap();

        drop(spi_data);

        let all_groups_valid = [&data_a, &data_b, &data_c, &data_d]
            .into_iter()
            .all(|data| data[6..8] == self.calculate_pec(&data[..6]));

        let mut cells: [u16; 12] = [0; 12];
        cells[0] = ((data_a[1] as u16) << 8) | (data_a[0] as u16);
        cells[1] = ((data_a[3] as u16) << 8) | (data_a[2] as u16);
        cells[2] = ((data_a[5] as u16) << 8) | (data_a[4] as u16);
        cells[3] = ((data_b[1] as u16) << 8) | (data_b[0] as u16);
        cells[4] = ((data_b[3] as u16) << 8) | (data_b[2] as u16);
        cells[5] = ((data_b[5] as u16) << 8) | (data_b[4] as u16);
        cells[6] = ((data_c[1] as u16) << 8) | (data_c[0] as u16);
        cells[7] = ((data_c[3] as u16) << 8) | (data_c[2] as u16);
        cells[8] = ((data_c[5] as u16) << 8) | (data_c[4] as u16);
        cells[9] = ((data_d[1] as u16) << 8) | (data_d[0] as u16);
        cells[10] = ((data_d[3] as u16) << 8) | (data_d[2] as u16);
        cells[11] = ((data_d[5] as u16) << 8) | (data_d[4] as u16);

        let mut bms_data = self.bms.lock().await;
        for (i, cell) in cells.into_iter().enumerate() {
            bms_data.update_cell(i, cell);
        }
        bms_data.set_cell_data_valid(all_groups_valid);
        drop(bms_data);

        Ok(())
    }

    pub async fn start_temperature_conversion(&mut self) -> Result<(), ()> {
        let cmd = self.prepare_command(ADAX);
        self.wakeup().await;
        let mut spi_data = self.spi.lock().await;
        spi_data.write(&cmd).await;
        drop(spi_data);

        Timer::after_millis(1).await;

        let poll = self.prepare_command(PLADC);
        loop {
            let mut spi_data = self.spi.lock().await;
            let mut status = [0u8; 8];
            spi_data.cmd_read(&poll, &mut status).await.unwrap();
            if status[0] & 0x01 != 0 {
                break;
            }
            drop(spi_data);
            Timer::after(Duration::from_micros(500)).await;
        }

        Timer::after_millis(1).await;
        Ok(())
    }

    pub fn parse_temp(&self, voltage_gpio: u16, voltage_ref: u16) -> u16 {
        if voltage_gpio == 0 || voltage_ref <= voltage_gpio {
            return u16::MAX;
        }

        let r_th = (THERMISTOR_PULLUP_OHM as f32) * (voltage_gpio as f32)
            / ((voltage_ref - voltage_gpio) as f32);

        let inv_t =
            1f32 / (KELVIN_2_CELSIUS + 25f32) + (1f32 / B_COEFF) * logf((r_th / 1000f32) / R25);

        if inv_t < 0.0f32 {
            return u16::MIN;
        }

        let temp = if inv_t != 0.0f32 {
            1.0f32 / inv_t
        } else {
            1.0f32 / (inv_t + 1e-6)
        };

        let temp_i32: i32 = roundf((temp - KELVIN_2_CELSIUS) * 10.0f32) as i32;
        if temp_i32 < (MIN_TEMP as i32) {
            MIN_TEMP
        } else if temp_i32 > (MAX_TEMP as i32) {
            MAX_TEMP
        } else {
            temp_i32 as u16
        }
    }

    pub async fn select_mux_channel(&mut self, mux_index: u8, channel: u8) -> Result<(), ()> {
        if mux_index as usize >= MUXES || channel as usize >= CHANNELS_PER_MUX {
            return Err(());
        }

        let address = if mux_index == 0 { 0x90u8 } else { 0x94u8 };
        let control = 0x08 | (channel & 0x07);
        let comm = [
            0x60 | (address >> 4),
            (address << 4) | 0x08,
            control >> 4,
            (control << 4) | 0x09,
            0xFF,
            0xFF,
        ];

        let cmd_wr = self.prepare_command(WRCOMM);
        let mut frame_wr = [0u8; 12];
        frame_wr[0..4].copy_from_slice(&cmd_wr);
        frame_wr[4..10].copy_from_slice(&comm);
        frame_wr[10..12].copy_from_slice(&self.calculate_pec(&comm));

        self.wakeup_idle().await;
        let mut spi_data = self.spi.lock().await;
        spi_data.write(&frame_wr).await;

        let cmd_exec = self.prepare_command(STCOMM);
        spi_data.start_comm(&cmd_exec).await?;

        drop(spi_data);
        Timer::after_millis(1).await;
        Ok(())
    }

    pub async fn read_mux_temperatures(&mut self) -> Result<(), ()> {
        for therm_idx in 0..NUM_THERMISTORS {
            let mux_index = (therm_idx / CHANNELS_PER_MUX) as u8;
            let channel = (therm_idx % CHANNELS_PER_MUX) as u8;

            if self.select_mux_channel(mux_index, channel).await.is_err() {
                continue;
            }

            if self.start_temperature_conversion().await.is_err() {
                continue;
            }

            self.wakeup_idle().await;
            let mut spi_data = self.spi.lock().await;

            let mut auxa = [0u8; 8];
            let cmd_a = self.prepare_command(RDAUXA);
            spi_data.cmd_read(&cmd_a, &mut auxa).await.unwrap();

            let mut auxb = [0u8; 8];
            let cmd_b = self.prepare_command(RDAUXB);
            spi_data.cmd_read(&cmd_b, &mut auxb).await.unwrap();

            drop(spi_data);

            let code = u16::from_be_bytes([auxa[1], auxa[0]]);
            let vref = u16::from_be_bytes([auxb[5], auxb[4]]);

            let mut bms = self.bms.lock().await;
            bms.update_temp(therm_idx, self.parse_temp(code, vref));
            drop(bms);

            Timer::after_millis(2).await;
        }

        Ok(())
    }

    pub async fn read_temperatures(&mut self) -> Result<(), ()> {
        self.read_mux_temperatures().await
    }

    pub async fn update(&mut self) -> Result<(), ()> {
        self.set_mode(MODE::NORMAL).await;

        {
            let mut bms_data = self.bms.lock().await;
            bms_data.begin_measurement_cycle();
        }

        self.read_cell_voltages().await?;
        self.read_temperatures().await?;

        let mut bms_data = self.bms.lock().await;
        bms_data.update();
        drop(bms_data);

        Ok(())
    }

    pub async fn check_need_balance(&self) -> bool {
        let bms_data = self.bms.lock().await;
        for i in 0..NUM_CELLS {
            if (bms_data.cell_volts(i) as i16 - bms_data.min_volt() as i16) > BAL_EPSILON {
                return true;
            }
        }
        false
    }
}
