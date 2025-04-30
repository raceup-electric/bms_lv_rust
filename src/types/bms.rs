use libm::roundf;

pub static NUM_CELLS: usize = 12;
pub static NUM_TERMISTORS: usize = 4;

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

    pub fn reset(&mut self) {
        *self = Self::new();
    }
}