use libm::roundf;

pub static NUM_CELLS: usize = 12;

#[derive(Default, Debug, Copy, Clone)]
pub struct BMS {
    pub cell_volts: [u16; NUM_CELLS],
    tot_volt: u32,
    max_volt: u16,
    min_volt: u16,
    avg_volt: u16,
    temp: u16,
}

impl BMS {
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

    pub fn update_temp(&mut self, temp: u16) {
        self.temp = temp;
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
        let v_float = (self.tot_volt as f32) /12.0f32;
        let rounded: u16 = if v_float >= 0.0 {
            roundf(v_float).max(0.0) as u16
        } else {
            0
        };

        self.avg_volt = rounded;

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

    pub fn temp(&self) -> u16 {
        self.temp
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }
}