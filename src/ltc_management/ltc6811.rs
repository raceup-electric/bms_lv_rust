use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_time::{Duration, Timer};
use crate::types::bms::BMS;
use super::spi_device::SpiDevice;

// LTC6811 Commands
const WRCFGA: [u8; 2] = [0x00, 0x01]; // Write Configuration Register Group A
const RDCFGA: [u8; 2] = [0x00, 0x02]; // Read Configuration Register Group A
const RDCVA: [u8; 2] = [0x00, 0x04];  // Read Cell Voltage Register Group A (cells 1-3)
const RDCVB: [u8; 2] = [0x00, 0x06];  // Read Cell Voltage Register Group B (cells 4-6)
const RDCVC: [u8; 2] = [0x00, 0x08];  // Read Cell Voltage Register Group C (cells 7-9)
const RDCVD: [u8; 2] = [0x00, 0x0A];  // Read Cell Voltage Register Group D (cells 10-12)
const RDAUXA: [u8; 2] = [0x00, 0x0C]; // Read Auxiliary Register Group A (for temperature)
const ADCV: [u8; 2] = [0x02, 0x60];   // Start Cell Voltage ADC Conversion and Poll Status
const ADAX: [u8; 2] = [0x04, 0x60];   // Start Temperature Conversion and Poll Status

// Thresholds and balancing parameters (example values – adjust as required)
const UV_THRESHOLD: u16 = 3000; // in millivolts
const OV_THRESHOLD: u16 = 4200; // in millivolts
const BAL_EPSILON: u16 = 50;    // allowable voltage difference for balancing

// Configuration
const _NUM_CELLS: usize = 12;
const REFON: u8 = 0x04;      // Reference Powered Up
const ADCOPT: u8 = 0x01;     // ADC Mode option bit
// GPIO configuration bits if needed
const GPIO1: u8 = 0x01;      // GPIO1 as digital input
const GPIO2: u8 = 0x01;      // GPIO2 as digital input
const GPIO3: u8 = 0x01;      // GPIO3 as digital input
const GPIO4: u8 = 0x01;      // GPIO4 as digital input
const GPIO5: u8 = 0x01;      // GPIO5 as digital input
const GPIOS: u8 = 0x0 | (GPIO1 << 3) | (GPIO2 << 4) | (GPIO3 << 5) | (GPIO4 << 6) | (GPIO5 << 7);

#[derive(PartialEq)]
pub enum MODE {
    NORMAL,
    BALANCING,
    SLEEP
}

// LTC6811 Management structure
pub struct LTC6811 {
    spi: &'static Mutex<CriticalSectionRawMutex, SpiDevice<'static>>,
    bms: &'static Mutex<CriticalSectionRawMutex, BMS>,
    config: [u8; 6],  // Configuration registers
    mode: MODE
}

impl LTC6811 {
    pub async fn new(spi: &'static Mutex<CriticalSectionRawMutex, SpiDevice<'static>>,
                     bms: &'static Mutex<CriticalSectionRawMutex, BMS>,
) -> Self {
        // Initialize with default configuration
        // CFGR0: ADCOPT | GPIO[5:1]
        // CFGR1: Reserved | Reserved
        // CFGR2: REFON | Reserved
        // CFGR3: Reserved
        // CFGR4: Cell discharge timer and under-voltage comparison enable
        // CFGR5: Cell discharge timer and over-voltage comparison enable
        let config = [
            0x00,
            0x00,
            REFON,
            0x00,
            0x00,
            0x00,
        ];

        LTC6811 {
            spi,
            bms,
            config,
            mode: MODE::NORMAL
        }
    }

    // Calculate PEC (CRC) for LTC6811 communication
    fn calculate_pec(data: &[u8]) -> [u8; 2] {
        let mut crc = 0x0010; // Initial CRC value
        for byte in data {
            crc ^= (*byte as u16) << 8;
            for _ in 0..8 {
                if crc & 0x8000 != 0 {
                    crc = (crc << 1) ^ 0x4599;
                } else {
                    crc = crc << 1;
                }
            }
        }
        [(crc >> 8) as u8, crc as u8]
    }

    pub fn set_mode(&mut self, mode: MODE) {
        self.mode = mode;
    }

    // Prepare command with PEC
    fn prepare_command(&self, cmd: [u8; 2]) -> [u8; 4] {
        let mut command = [0u8; 4];
        command[0] = cmd[0];
        command[1] = cmd[1];
        let pec = Self::calculate_pec(&cmd);
        command[2] = pec[0];
        command[3] = pec[1];
        command
    }

    pub async fn init_cfg(&mut self) -> Result<(), ()> {
        let uv_val = (UV_THRESHOLD /16) -1;
        let ov_val = OV_THRESHOLD /16;

        self.config[0] = GPIOS | ADCOPT;
        self.config[1] = (uv_val & 0xFF) as u8;
        self.config[2] = (((ov_val & 0xF) << 4) | ((uv_val & 0xF00) >> 8)) as u8;
        self.config[3] = (ov_val >> 4) as u8;
        {
            let bms_data = self.bms.lock().await;
            // Assume bms_data.min_volt and bms_data.max_volt are set when valid.
            if self.mode == MODE::BALANCING && bms_data.min_volt() != 0 && bms_data.max_volt() != 0 {
                let mut discharge_bitmap: u16 = 0;
                // Iterate over all 12 cells. Here we assume that bms_data.cell_volts is an array of 12 u16.
                for i in 0.._NUM_CELLS {
                    // If the cell voltage exceeds the minimum by more than BAL_EPSILON, enable discharge.
                    if (bms_data.cell_volts[i] as i16 - bms_data.min_volt() as i16) > BAL_EPSILON as i16 {
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
        spi_data.write(&cmd).await;
        self.transfer_ltc(&mut spi_data, &mut read_config).await;
        drop(spi_data);
        
        // Config verification could be done here if needed
        
        Ok(())
    }

    pub async fn wakeup(&mut self) {
        let mut spi_data = self.spi.lock().await;
        spi_data.cs.set_low();

        for _ in 0..50 {
            spi_data.write(&[0xff]).await;
            embassy_time::Timer::after_millis(5).await;
        }

        spi_data.cs.set_high();
        drop(spi_data);
    }

    pub async fn wakeup_idle(&mut self) {
        let mut spi_data = self.spi.lock().await;
        spi_data.cs.set_low();
        spi_data.write(&[0xFF]).await;
        spi_data.cs.set_high();
        drop(spi_data);
    }
    
    
    // Write configuration to LTC6811
    pub async fn write_config(&mut self) -> Result<(), ()> {
        let cmd = self.prepare_command(WRCFGA);
        
        // Prepare data packet with PEC
        let mut data = [0u8; 8];
        data[0..6].copy_from_slice(&self.config);
        let pec = Self::calculate_pec(&self.config);
        data[6] = pec[0];
        data[7] = pec[1];

        self.wakeup_idle().await;
        let mut spi_data = self.spi.lock().await;
        // Send command
        spi_data.write(&cmd).await;
        // Send data
        spi_data.write(&data).await;
        
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
    
    // Helper function according to tommy's code
    pub async fn transfer_ltc(&self, spi_data: &mut SpiDevice<'static>, rx_buffer: &mut [u8]) {
        for byte in rx_buffer {
            match spi_data.transfer(&[0xFF], &mut [*byte]).await {
                Ok(_) => continue,

                Err(_) => defmt::info!("Error Reading SPI")
            }
        }
    }
    
    // Read cell voltage registers and update BMS
    pub async fn read_cell_voltages(&mut self) -> Result<(), ()> {
        // Start voltage conversion
        self.start_cell_conversion().await?;
        
        let mut spi_data = self.spi.lock().await;

        // Read voltage registers (cells 1-3)
        let cmd_a = self.prepare_command(RDCVA);
        let mut data_a = [0u8; 8]; // 6 data bytes + 2 PEC bytes        
        spi_data.write(&cmd_a).await;
        self.transfer_ltc(&mut spi_data, &mut data_a).await;
        
        // Read voltage registers (cells 4-6)
        let cmd_b = self.prepare_command(RDCVB);
        let mut data_b = [0u8; 8];
        spi_data.write(&cmd_b).await;
        self.transfer_ltc(&mut spi_data, &mut data_b).await;
        
        // Read voltage registers (cells 7-9)
        let cmd_c = self.prepare_command(RDCVC);
        let mut data_c = [0u8; 8];
        spi_data.write(&cmd_c).await;
        self.transfer_ltc(&mut spi_data, &mut data_c).await;
        
        // Read voltage registers (cells 10-12)
        let cmd_d = self.prepare_command(RDCVD);
        let mut data_d = [0u8; 8];
        spi_data.write(&cmd_d).await;
        self.transfer_ltc(&mut spi_data, &mut data_d).await;
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
        Timer::after(Duration::from_millis(6)).await;
        
        Ok(())
    }

    // Read temperature sensor (assuming connected to GPIO1/AUX1)
    pub async fn read_temperature(&mut self) -> Result<(), ()> {
        self.start_temperature_conversion().await?;

        // Wait for conversion to complete
        Timer::after(Duration::from_millis(6)).await;
        
        let mut spi_data = self.spi.lock().await;
        // Read auxiliary registers
        let cmd_aux = self.prepare_command(RDAUXA);
        let mut data_aux = [0u8; 8];
        spi_data.write(&cmd_aux).await;
        self.transfer_ltc(&mut spi_data, &mut data_aux).await;
        
        // Extract temperature value (assuming connected to GPIO1)
        let temp = ((data_aux[1] as u16) << 8) | (data_aux[0] as u16);
        
        let mut bms_data: embassy_sync::mutex::MutexGuard<'_, CriticalSectionRawMutex, BMS> = self.bms.lock().await;
        bms_data.update_temp(temp);
        drop(bms_data);
        
        Ok(())
    }


    // Periodic update - call this regularly to keep BMS data fresh
    pub async fn update(&mut self) -> Result<(), ()> {
        let mut bms_data = self.bms.lock().await;
        bms_data.reset();
        drop(bms_data);
        // Read all cell voltages
        match self.read_cell_voltages().await {
            Ok(_) => {},
            Err(_) => return Err(())
        }
        
        // Read temperature
        self.read_temperature().await
    }
    
    // Balance cells if needed
    pub async fn _balance_cells(&mut self, threshold: u16) -> Result<(), ()> {

        let bms_data: embassy_sync::mutex::MutexGuard<'_, CriticalSectionRawMutex, BMS> = self.bms.lock().await;

        // Get current cell data
        let max_volt = bms_data.max_volt();
        
        // For each cell, check if it needs balancing
        for i in 0.._NUM_CELLS {
            let cell_volt = bms_data.cell_volts[i];
            
            // If this cell's voltage is above threshold compared to minimum,
            // enable its discharge circuit
            if max_volt - cell_volt > threshold {
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