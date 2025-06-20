use libm::roundf;

pub static NUM_CELLS: usize = 12;
pub static NUM_TERMISTORS: usize = 4;
pub static NUM_HISTORY: usize = 5;

#[derive(Default, Debug, Copy, Clone)]
pub struct SLAVEBMS {
    bms_history: [BMS; NUM_HISTORY],
    index: usize,
    tot_volt: u32,
    max_volt: u16,
    min_volt: u16,
    avg_volt: u16,
    max_temp: u16,
    min_temp: u16,
    avg_temp: u16,
}

#[derive(Default, Debug, Copy, Clone)]
pub struct BMS {
    pub cell_volts: [u16; NUM_CELLS],
    tot_volt: u32,
    max_volt: u16,
    min_volt: u16,
    avg_volt: u16,
    pub temperatures: [u16; NUM_TERMISTORS],
    max_temp: u16,
    min_temp: u16,
    avg_temp: u16,
}

impl BMS {
    pub fn new() -> Self {
        BMS {
            cell_volts: [0; NUM_CELLS],
            max_volt: 0,
            min_volt: 0,
            avg_volt: 0,
            tot_volt: 0,
            temperatures: [0; NUM_TERMISTORS],
            max_temp: 0,
            min_temp: 0,
            avg_temp: 0,
        }
    }

    pub fn update_temp(&mut self, i: usize, value: u16) {
        self.temperatures[i] = value;
        self.update();
    }

    pub fn update_cell(&mut self, i: usize, value: u16) {
        self.cell_volts[i] = value;
        self.update();
    }

    fn update(&mut self){
        self.tot_volt = 0;
        self.max_volt = 0;
        self.min_volt = u16::MAX;
        for &volt in self.cell_volts.iter() {
            self.tot_volt = self.tot_volt.wrapping_add(volt as u32);
            self.max_volt = if volt > self.max_volt {volt} else {self.max_volt};
            self.min_volt = if volt < self.min_volt {volt} else {self.min_volt};
        }
        let v_float = (self.tot_volt as f32) /(NUM_CELLS as f32);
        let rounded: u16 = if v_float >= 0.0 {
            roundf(v_float).max(0.0) as u16
        } else {
            0
        };

        self.avg_volt = rounded;

        let mut tot_temp: u32 = 0;
        self.max_temp = 0;
        self.min_temp = u16::MAX;
        for &temp in self.temperatures.iter() {
            tot_temp = tot_temp.wrapping_add(temp as u32);
            self.max_temp = if temp > self.max_temp {temp} else {self.max_temp};
            self.min_temp = if temp < self.min_temp {temp} else {self.min_temp};

        }
        let v_float = (tot_temp as f32) /(NUM_TERMISTORS as f32);
        let rounded: u16 = if v_float >= 0.0 {
            roundf(v_float).max(0.0) as u16
        } else {
            0
        };

        self.avg_temp = rounded;


    }

    pub fn avg_volt(&self) -> u16 {
        self.avg_volt
    }

    pub fn tot_volt(&self) -> u32 {
        self.tot_volt
    }

    pub fn min_volt(&self) -> u16 {
        self.min_volt
    }

    pub fn max_volt(&self) -> u16 {
        self.max_volt
    }


    pub fn avg_temp(&self) -> u16 {
        self.avg_temp
    }

    pub fn min_temp(&self) -> u16 {
        self.min_temp
    }

    pub fn max_temp(&self) -> u16 {
        self.max_temp
    }
}

impl SLAVEBMS {
    pub fn new() -> Self {
        let bms_history = [BMS::new(); NUM_HISTORY];
        SLAVEBMS {
            bms_history,
            index: 0 as usize,
            tot_volt: 0,
            max_volt: 0,
            min_volt: 0,
            avg_volt: 0,
            max_temp: 0,
            min_temp: 0,
            avg_temp: 0
        }
    }

    pub fn update(&mut self) {
        let mut tot_volt: u64 = 0;
        let mut max_volt: u64 = 0;
        let mut min_volt: u64 = 0;
        let mut avg_volt: u64 = 0;
        let mut max_temp: u64 = 0;
        let mut min_temp: u64 = 0;
        let mut avg_temp: u64 = 0;

        for &bms in self.bms_history.iter() {
            tot_volt = tot_volt.wrapping_add(bms.tot_volt() as u64);
            max_volt = max_volt.wrapping_add(bms.max_volt() as u64);
            min_volt = min_volt.wrapping_add(bms.min_volt() as u64);
            avg_volt = avg_volt.wrapping_add(bms.avg_volt() as u64);
            max_temp = max_temp.wrapping_add(bms.max_temp() as u64);
            min_temp = min_temp.wrapping_add(bms.min_temp() as u64);
            avg_temp = avg_temp.wrapping_add(bms.avg_temp() as u64);
        }

        let tot_v_float: f32 = ((tot_volt as f64) /(NUM_HISTORY as f64) ) as f32; 
        self.tot_volt = if tot_v_float >= 0.0 {
            roundf(tot_v_float).max(0.0) as u32
        } else {
            0
        };

        let max_v_float: f32 = ((max_volt as f64) /(NUM_HISTORY as f64) ) as f32; 
        self.max_volt = if max_v_float >= 0.0 {
            roundf(max_v_float).max(0.0) as u16
        } else {
            0
        };

        let min_v_float: f32 = ((min_volt as f64) /(NUM_HISTORY as f64) ) as f32; 
        self.min_volt = if min_v_float >= 0.0 {
            roundf(min_v_float).max(0.0) as u16
        } else {
            0
        };

        let avg_v_float: f32 = ((avg_volt as f64) /(NUM_HISTORY as f64) ) as f32; 
        self.avg_volt = if avg_v_float >= 0.0 {
            roundf(avg_v_float).max(0.0) as u16
        } else {
            0
        };

        let max_t_float: f32 = ((max_temp as f64) /(NUM_HISTORY as f64) ) as f32; 
        self.max_temp = if max_t_float >= 0.0 {
            roundf(max_t_float).max(0.0) as u16
        } else {
            0
        };

        let min_t_float: f32 = ((min_temp as f64) /(NUM_HISTORY as f64) ) as f32; 
        self.min_temp = if min_t_float >= 0.0 {
            roundf(min_t_float).max(0.0) as u16
        } else {
            0
        };

        let avg_t_float: f32 = ((avg_temp as f64) /(NUM_HISTORY as f64) ) as f32; 
        self.avg_temp = if avg_t_float >= 0.0 {
            roundf(avg_t_float).max(0.0) as u16
        } else {
            0
        };

        self.index = self.index + 1;
        if self.index >= NUM_HISTORY {
            self.index = 0;
        }
    }

    pub fn update_temp(&mut self, i: usize, value: u16) {
        self.bms_history[self.index].update_temp(i, value);
    }

    pub fn update_cell(&mut self, i: usize, value: u16) {
        self.bms_history[self.index].update_cell(i, value);
    }

    pub fn avg_volt(&self) -> u16 {
        self.avg_volt
    }

    pub fn tot_volt(&self) -> u32 {
        self.tot_volt
    }

    pub fn min_volt(&self) -> u16 {
        self.min_volt
    }

    pub fn max_volt(&self) -> u16 {
        self.max_volt
    }

    pub fn avg_temp(&self) -> u16 {
        self.avg_temp
    }

    pub fn min_temp(&self) -> u16 {
        self.min_temp
    }

    pub fn max_temp(&self) -> u16 {
        self.max_temp
    }

    pub fn cell_volts(&self, i: usize) -> u16 {
        self.bms_history[self.index].cell_volts[i]
    }
}