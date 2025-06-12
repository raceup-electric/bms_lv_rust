use super::spi_device::SpiDevice;
use crate::types::{bms::SLAVEBMS, VOLTAGES};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_time::{Duration, Timer};

use libm::{logf, roundf};

pub const WRCFGA: [u8; 2] = [0x00, 0x01];

/// Read Configuration Register Group A
pub const RDCFGA: [u8; 2] = [0x00, 0x02];

/// Read Cell Voltage Register Group A (cells 1-3)
pub const RDCVA: [u8; 2] = [0x00, 0x04];

/// Read Cell Voltage Register Group B (cells 4-6)
pub const RDCVB: [u8; 2] = [0x00, 0x06];

/// Read Cell Voltage Register Group C (cells 7-9)
pub const RDCVC: [u8; 2] = [0x00, 0x08];

/// Read Cell Voltage Register Group D (cells 10-12)
pub const RDCVD: [u8; 2] = [0x00, 0x0A];

/// Read Auxiliary Register Group A (for temperature)
pub const RDAUXA: [u8; 2] = [0x00, 0x0C];

pub const RDAUXB: [u8; 2] = [0x00, 0x0E];

pub const ADCV: [u8; 2] = [0x02, 0x60];

pub const ADAX: [u8; 2] = [0x05, 0x60];

// Tensione di riferimento dell'ADC (in millivolt).
// Esempio: se usi VREF = 3.3 V con scala 12 bit, VREF2_MV = 3300
const VREF2_MV: u32 = 3300;

// Resistenza di pull-down fissa (in ohm) del termistore.
// Esempio: R_THERMISTOR = 10 000 Ω (10 kΩ)
const RTHERMISTOR_OHM: u32 = 10_000;

// Coefficiente Beta del termistore (in Kelvin)
const B_COEFF: f32     = 3950.0;
// Conversione da Kelvin a Celsius
const KELVIN_2_CELSIUS: f32 = 273.15;

// Valori speciali di saturazione / guasto
const MAX_TEMP: u16 = u16::MAX;  // OverTemp (corto a massa)
const MIN_TEMP: u16 = 0;      
// Thresholds and balancing parameters (example values – adjust as required)\
const BAL_EPSILON: u16 = 50; // allowable voltage difference for balancing

// Configuration
const NUM_CELLS: usize = 12;
const REFON: u8 = 0x00; // Reference Powered Up
const ADCOPT: u8 = 0x00; // ADC Mode option bit
                         // GPIO configuration bits if needed
const GPIO1: u8 = 0x01; // GPIO1 as digital input
const GPIO2: u8 = 0x01; // GPIO2 as digital input
const GPIO3: u8 = 0x01; // GPIO3 as digital input
const GPIO4: u8 = 0x01; // GPIO4 as digital input
const GPIO5: u8 = 0x01; // GPIO5 as digital input
const GPIOS: u8 = 0x0 | (GPIO1 << 3) | (GPIO2 << 4) | (GPIO3 << 5) | (GPIO4 << 6) | (GPIO5 << 7);

#[allow(unused)]
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

#[derive(PartialEq, Clone)]
pub enum MODE {
    NORMAL,
    BALANCING,
}

// LTC6811 Management structure
pub struct LTC6811 {
    spi: &'static Mutex<CriticalSectionRawMutex, SpiDevice<'static>>,
    bms: &'static Mutex<CriticalSectionRawMutex, SLAVEBMS>,
    config: [u8; 6], // Configuration registers
    mode: MODE,
}

impl LTC6811 {
    pub async fn new(
        spi: &'static Mutex<CriticalSectionRawMutex, SpiDevice<'static>>,
        bms: &'static Mutex<CriticalSectionRawMutex, SLAVEBMS>,
    ) -> Self {
        // Initialize with default configuration
        // CFGR0: ADCOPT | GPIO[5:1]
        // CFGR1: Reserved | Reserved
        // CFGR2: REFON | Reserved
        // CFGR3: Reserved
        // CFGR4: Cell discharge timer and under-voltage comparison enable
        // CFGR5: Cell discharge timer and over-voltage comparison enable
        let config = [0x00, 0x00, REFON, 0x00, 0x00, 0x00];

        LTC6811 {
            spi,
            bms,
            config,
            mode: MODE::NORMAL,
        }
    }

    // Calculate PEC (CRC) for LTC6811 communication
    pub fn calculate_pec(&self, data: &[u8]) -> [u8; 2] {
        let mut remainder: u16 = 16;

        for byte in data {
            let address: usize = (((remainder >> 7) ^ (*byte as u16)) & 0xff).into();
            remainder = (remainder << 8) ^ CRC15_TABLE[address];
        }

        // The CRC15 has a 0 in the LSB
        remainder <<= 1;

        [(remainder >> 8) as u8, remainder as u8]
    }

    pub async fn set_mode(&mut self, mode: MODE) {
        if self.mode != mode {
            self.mode = mode;
            let _ = self.init_cfg().await;
        }
    }

    pub fn get_mode(&self) -> MODE {
        self.mode.clone()
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

        self.config[0] = GPIOS | ADCOPT;
        self.config[1] = (uv_val & 0xFF) as u8;
        self.config[2] = (((ov_val & 0xF) << 4) | ((uv_val & 0xF00) >> 8)) as u8;
        self.config[3] = (ov_val >> 4) as u8;
        {
            let bms_data = self.bms.lock().await;
            // Assume bms_data.min_volt and bms_data.max_volt are set when valid.
            if self.mode == MODE::BALANCING && bms_data.min_volt() != 0 && bms_data.max_volt() != 0
            {
                let mut discharge_bitmap: u16 = 0;
                // Iterate over all 12 cells. Here we assume that bms_data.cell_volts is an array of 12 u16.
                for i in 0..NUM_CELLS {
                    // If the cell voltage exceeds the minimum by more than BAL_EPSILON, enable discharge.
                    if (bms_data.cell_volts(i) as i16 - bms_data.min_volt() as i16)
                        > BAL_EPSILON as i16
                    {
                        discharge_bitmap |= 1 << i;
                    }
                }
                // In the C code the lower 8 bits go into config[4] and the upper nibble (4 bits) goes into config[5].
                self.config[4] = (discharge_bitmap & 0xFF) as u8;
                self.config[5] = ((discharge_bitmap >> 8) & 0x0F) as u8;
            } else {
                // Not balancing (or no measurements available): clear discharge bits.
                self.config[4] = 0x00;
                self.config[5] = 0x00;
            }
        }

        // Write the configuration to the chip.
        self.write_config().await?;
        Ok(())
    }

    // Initialize the LTC6811
    pub async fn init(&mut self) -> Result<(), ()> {
        // Write configuration registers
        self.init_cfg().await?;

        self.wakeup().await;
        // Delay to allow LTC6811 to stabilize
        Timer::after(Duration::from_millis(10)).await;

        // Verify configuration
        let mut read_config = [0u8; 8]; // 6 config bytes + 2 PEC bytes
        let cmd = self.prepare_command(RDCFGA);
        let mut spi_data = self.spi.lock().await;
        // spi_data.write(&cmd).await;
        // self.transfer_ltc(&mut spi_data, &mut read_config).await;
        spi_data.cmd_read(&cmd, &mut read_config).await.unwrap();
        drop(spi_data);

        // Config verification could be done here if needed

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

    // Write configuration to LTC6811
    pub async fn write_config(&mut self) -> Result<(), ()> {
        let cmd = self.prepare_command(WRCFGA);

        // Prepare data packet with PEC
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
        // Send command
        spi_data.write(&cmd_final).await;
        drop(spi_data);
        Ok(())
    }

    // Start cell voltage conversion
    pub async fn start_cell_conversion(&mut self) -> Result<(), ()> {
        let cmd = self.prepare_command(ADCV);

        self.wakeup_idle().await;
        let mut spi_data = self.spi.lock().await;
        // Send command
        spi_data.write(&cmd).await;

        drop(spi_data);
        // Wait for conversion to complete (typical conversion time ~2ms)
        Timer::after(Duration::from_millis(6)).await;

        Ok(())
    }
    // Read cell voltage registers and update BMS
    pub async fn read_cell_voltages(&mut self) -> Result<(), ()> {
        // Start voltage conversion
        self.start_cell_conversion().await?;

        self.wakeup_idle().await;
        let mut spi_data = self.spi.lock().await;

        // Read voltage registers (cells 1-3)
        let cmd_a = self.prepare_command(RDCVA);
        let mut data_a = [0u8; 8]; // 6 data bytes + 2 PEC bytes
                                   // spi_data.write(&cmd_a).await;
                                   // self.transfer_ltc(&mut spi_data, &mut data_a).await;
        spi_data.cmd_read(&cmd_a, &mut data_a).await.unwrap();
        // Read voltage registers (cells 4-6)
        let cmd_b = self.prepare_command(RDCVB);
        let mut data_b = [0u8; 8];
        // spi_data.write(&cmd_b).await;
        // self.transfer_ltc(&mut spi_data, &mut data_b).await;
        spi_data.cmd_read(&cmd_b, &mut data_b).await.unwrap();

        // Read voltage registers (cells 7-9)
        let cmd_c = self.prepare_command(RDCVC);
        let mut data_c = [0u8; 8];
        // spi_data.write(&cmd_c).await;
        // self.transfer_ltc(&mut spi_data, &mut data_c).await;
        spi_data.cmd_read(&cmd_c, &mut data_c).await.unwrap();

        // Read voltage registers (cells 10-12)
        let cmd_d = self.prepare_command(RDCVD);
        let mut data_d = [0u8; 8];
        // spi_data.write(&cmd_d).await;
        // self.transfer_ltc(&mut spi_data, &mut data_d).await;
        spi_data.cmd_read(&cmd_d, &mut data_d).await.unwrap();

        drop(spi_data);

        // Process and update BMS with cell voltages
        // Each cell voltage is 16-bit (2 bytes)

        let mut cells: [u16; 12] = [0; 12];
        // Cells 1-3
        cells[0] = ((data_a[1] as u16) << 8) | (data_a[0] as u16);
        cells[1] = ((data_a[3] as u16) << 8) | (data_a[2] as u16);
        cells[2] = ((data_a[5] as u16) << 8) | (data_a[4] as u16);

        // Cells 4-6
        cells[3] = ((data_b[1] as u16) << 8) | (data_b[0] as u16);
        cells[4] = ((data_b[3] as u16) << 8) | (data_b[2] as u16);
        cells[5] = ((data_b[5] as u16) << 8) | (data_b[4] as u16);

        // Cells 7-9
        cells[6] = ((data_c[1] as u16) << 8) | (data_c[0] as u16);
        cells[7] = ((data_c[3] as u16) << 8) | (data_c[2] as u16);
        cells[8] = ((data_c[5] as u16) << 8) | (data_c[4] as u16);

        // Cells 10-12
        cells[9] = ((data_d[1] as u16) << 8) | (data_d[0] as u16);
        cells[10] = ((data_d[3] as u16) << 8) | (data_d[2] as u16);
        cells[11] = ((data_d[5] as u16) << 8) | (data_d[4] as u16);

        // Update BMS with cell voltages
        let mut bms_data = self.bms.lock().await;

        for i in 0..12 {
            bms_data.update_cell(i, cells[i]);
        }
        drop(bms_data);

        Ok(())
    }

    pub async fn start_temperature_conversion(&mut self) -> Result<(), ()> {
        let cmd = self.prepare_command(ADAX);
        self.wakeup_idle().await;
        let mut spi_data = self.spi.lock().await;
        // Send command
        spi_data.write(&cmd).await;

        drop(spi_data);
        // Wait for conversion to complete (typical conversion time ~2ms)
        Timer::after(Duration::from_millis(10)).await;

        Ok(())
    }

    // Read temperature sensor (assuming connected to GPIO1/AUX1)
    pub async fn read_temperatures(&mut self) -> Result<(), ()> {
        // 1) start the ADC on the GPIO pins
        self.start_temperature_conversion().await?;

        self.wakeup_idle().await;
        let mut spi_data = self.spi.lock().await;

        // lock SPI once
        let mut auxa = [0u8; 8];
        let cmd_a = self.prepare_command(RDAUXA);
        spi_data.cmd_read(&cmd_a, &mut auxa).await.unwrap();

        // 3) read AUXB (contains GPIO4)
        let mut auxb = [0u8; 8];
        let cmd_b = self.prepare_command(RDAUXB);
        spi_data.cmd_read(&cmd_b, &mut auxb).await.unwrap();
        // release SPI
        drop(spi_data);

        // 4) PEC check
        let pec_a = [auxa[6], auxa[7]];
        if pec_a != self.calculate_pec(&auxa[0..6]) {
            defmt::error!("PEC fail AUXA");
            //return Err(());
        }
        let pec_b = [auxb[6], auxb[7]];
        if pec_b != self.calculate_pec(&auxb[0..6]) {
            defmt::error!("PEC fail AUXB");
            //return Err(());
        }

        // 5) extract the four raw ADC codes
        let codes = [
            u16::from_be_bytes([auxa[0], auxa[1]]), // GPIO1
            u16::from_be_bytes([auxa[2], auxa[3]]), // GPIO2
            u16::from_be_bytes([auxa[4], auxa[5]]), // GPIO3
            u16::from_be_bytes([auxb[0], auxb[1]]), // GPIO4
        ];

        // 6) update your BMS struct
        let mut bms = self.bms.lock().await;
        for (i, &code) in codes.iter().enumerate() {
            bms.update_temp(i, self.parse_temp(code));
        }
        drop(bms);
        Ok(())
    }

    // Periodic update - call this regularly to keep BMS data fresh
    pub async fn update(&mut self) -> Result<(), ()> {
        // Read all cell voltages
        match self.read_cell_voltages().await {
            Ok(_) => {}
            Err(_) => return Err(()),
        }

        // Read temperature
        match self.read_temperatures().await {
            Ok(_) => {},
            Err(_) => return Err(()),
        }

        let mut bms_data = self.bms.lock().await;
        bms_data.update();
        drop(bms_data);

        Ok(())
    }

    pub fn parse_temp(&self, adc_raw: u16) -> u16 {
        // 1) SE raw == 0 → corto a massa, OverTemp:
        if adc_raw == 0 {
            return MAX_TEMP;
        }
        // 2) Se raw > ADC_MAX (impossibile, ma mettiamo un clamp):
        if adc_raw > 65534 {
            // Se davvero superiore, consideriamo termistore aperto:
            return adc_raw;
        }

        // 3) Converto raw ADC in millivolt:
        //    volt_mv = (adc_raw / ADC_MAX) * VREF2_MV
        // Usiamo un cast a f32 per non perdere precisione intermedia:
        
        let volt_mv_f: f32 = (adc_raw as f32) * 0.1; // 0.1 mV/LSB
        let volt_mv: u16 = roundf(volt_mv_f) as u16;
        
        // 4) Controlli di “circuito aperto”: se volt_mv >= VREF2_MV, termistore scollegato
        if (volt_mv as u32) >= VREF2_MV {
            return MIN_TEMP;
        }

        // 5) Calcolo la resistenza del termistore (in ohm) col partitore:
        //       Vout = adc_volt_mv, Vin = VREF2_MV
        //
        //   R_th = R_fixed * (Vout / (Vin - Vout))
        //
        // Passo a f32 per il calcolo con virgola mobile:
        let v_out: f32 = volt_mv as f32;
        let v_ref: f32 = VREF2_MV as f32;
        let r_fixed: f32 = RTHERMISTOR_OHM as f32;

        // Aggiungo un piccolo epsilon sul denominatore per evitare divisioni per zero
        let eps: f32 = 1e-6;
        let denom = v_ref - v_out;
        if denom <= 0.0 {
            // al netto del controllo precedente, questo non dovrebbe succedere,
            // ma se succede (per questioni di arrotondamento), la trattiamo come OPEN:
            return MIN_TEMP;
        }
        let r_th: f32 = r_fixed * v_out / (denom + eps);

        // 6) Calcolo della temperatura con Beta-Steinhart semplificato:
        //
        //    T(K) = B / ln(A * R_th)
        //    Temp(°C) = T(K) - 273.15
        //
        // In molti datasheet, A = 58.65 per un termistore da 10kΩ a 25°C.
        let a_coeff: f32 = 58.65_f32;
        let ln_arg = a_coeff * r_th;

        // Se ln_arg ≤ 0, logf restituisce NaN; lo trattiamo come temperatura minima (0°C):
        if ln_arg <= 0.0 {
            return MIN_TEMP;
        }
        let temp_kelvin: f32 = B_COEFF / logf(ln_arg);
        let temp_celsius: f32 = temp_kelvin - KELVIN_2_CELSIUS;

        // 7) Saturazione tra 0 e u16::MAX
        //    Prima passo a intero con arrotondamento,
        //    poi clamppo entro i limiti.
        let temp_i32: i32 = roundf(temp_celsius) as i32;
        if temp_i32 < (MIN_TEMP as i32) {
            MIN_TEMP
        } else if temp_i32 > (MAX_TEMP as i32) {
            MAX_TEMP
        } else {
            temp_i32 as u16
        }
    }

    // Balance cells if needed
    pub async fn balance_cells(&mut self) -> Result<(), ()> {
        let bms_data: embassy_sync::mutex::MutexGuard<'_, CriticalSectionRawMutex, SLAVEBMS> =
            self.bms.lock().await;

        // Get current cell data
        let min_volt: u16 = bms_data.min_volt();

        // For each cell, check if it needs balancing
        for i in 0..NUM_CELLS {
            let cell_volt = bms_data.cell_volts(i);

            // If this cell's voltage is above threshold compared to minimum,
            // enable its discharge circuit
            if cell_volt - min_volt > BAL_EPSILON {
                // Enable discharge for this cell by setting the appropriate bit in config
                // CFGR4 and CFGR5 control the discharge transistors
                // Cell 1-8 are in CFGR4, cells 9-12 are in CFGR5
                if i < 8 {
                    self.config[4] |= 1 << i;
                } else {
                    self.config[5] |= 1 << (i - 8);
                }
            } else {
                // Disable discharge for this cell
                if i < 8 {
                    self.config[4] &= !(1 << i);
                } else {
                    self.config[5] &= !(1 << (i - 8));
                }
            }
        }

        drop(bms_data);

        // Write the updated configuration to enable/disable balancing
        self.write_config().await?;

        Ok(())
    }
}
