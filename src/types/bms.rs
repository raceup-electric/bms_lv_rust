//! Battery Management System (BMS) module for monitoring and managing cell voltage and temperature.
//!
//! The `BMS` struct stores the voltage of individual cells, along with the total, average, minimum, and maximum
//! voltages. It also stores the temperature reading of the system.

use libm::roundf;

pub static NUM_CELLS: usize = 12;

/// Struct representing the Battery Management System (BMS).
/// It keeps track of individual cell voltages, total voltage, average voltage, 
/// maximum and minimum voltage, and temperature.
#[derive(Default, Debug, Copy, Clone)]
pub struct BMS {
    /// Array holding the voltage values for each cell in the system.
    pub cell_volts: [u16; NUM_CELLS],
    /// Total voltage of the cells.
    tot_volt: u32,
    /// Maximum voltage across all cells.
    max_volt: u16,
    /// Minimum voltage across all cells.
    min_volt: u16,
    /// Average voltage across all cells.
    avg_volt: u16,
    /// Temperature reading of the system.
    temp: u16,
}

impl BMS {
    /// Creates a new instance of the `BMS` struct with default values.
    ///
    /// # Returns
    ///
    /// A new `BMS` struct with all values initialized to defaults.
    pub fn new() -> Self {
        BMS {
            cell_volts: [0; NUM_CELLS],
            max_volt: 0,
            min_volt: 0,
            avg_volt: 0,
            tot_volt: 0,
            temp: 0,
        }
    }

    /// Updates the temperature of the system.
    ///
    /// # Arguments
    ///
    /// * `temp` - The temperature value to set for the system.
    pub fn update_temp(&mut self, temp: u16) {
        self.temp = temp;
    }

    /// Updates the voltage of a specific cell and recalculates the system's total, average,
    /// minimum, and maximum voltages.
    ///
    /// # Arguments
    ///
    /// * `i` - The index of the cell to update.
    /// * `value` - The new voltage value for the cell.
    pub fn update_cell(&mut self, i: usize, value: u16) {
        self.cell_volts[i] = value;
        self.update();
    }

    /// Recalculates the total, average, minimum, and maximum voltages after a cell voltage update.
    fn update(&mut self) {
        self.tot_volt = 0;
        self.max_volt = 0;
        self.min_volt = u16::MAX;
        for &volt in self.cell_volts.iter() {
            self.tot_volt = self.tot_volt.wrapping_add(volt as u32);
            self.max_volt = if volt > self.max_volt {volt} else {self.max_volt};
            self.min_volt = if volt < self.min_volt {volt} else {self.min_volt};
        }
        
        // Calculate the average voltage
        let v_float = (self.tot_volt as f32) / 12.0f32;
        let rounded: u16 = if v_float >= 0.0 {
            roundf(v_float).max(0.0) as u16
        } else {
            0
        };

        self.avg_volt = rounded;
    }

    /// Returns the average voltage of the system.
    ///
    /// # Returns
    ///
    /// The average voltage of the cells as a `u16`.
    pub fn avg_volt(&self) -> u16 {
        self.avg_volt
    }

    /// Returns the total voltage of the system.
    ///
    /// # Returns
    ///
    /// The total voltage of the cells as a `u32`.
    pub fn tot_volt(&self) -> u32 {
        self.tot_volt
    }

    /// Returns the minimum voltage across all cells.
    ///
    /// # Returns
    ///
    /// The minimum voltage as a `u16`.
    pub fn min_volt(&self) -> u16 {
        self.min_volt
    }

    /// Returns the maximum voltage across all cells.
    ///
    /// # Returns
    ///
    /// The maximum voltage as a `u16`.
    pub fn max_volt(&self) -> u16 {
        self.max_volt
    }

    /// Returns the current temperature reading of the system.
    ///
    /// # Returns
    ///
    /// The temperature as a `u16`.
    pub fn temp(&self) -> u16 {
        self.temp
    }

    /// Resets the BMS, clearing all stored voltage and temperature values.
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}
