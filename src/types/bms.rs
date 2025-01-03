pub static NUM_CELLS: usize = 12;

#[derive(Default, Debug, Copy, Clone)]
pub struct BMS {
    pub cell_volts: [u16; NUM_CELLS],
    index_cell: usize,
    tot_volt: u16,
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
            index_cell: 0
        }
    }

    pub fn update_cell(&mut self, i: usize, value: u16) {
        self.cell_volts[i] = value;
        self.update();
    }

    fn update(&mut self){
        self.tot_volt = 0;
        self.max_volt = 0;
        self.min_volt = u16::MAX;
        for _i in 0..11 {
            let x = self.get_next_cell_volt();
            self.tot_volt = self.tot_volt.wrapping_add(x);
            self.max_volt = if x > self.max_volt {x} else {self.max_volt};
            self.min_volt = if x < self.min_volt {x} else {self.min_volt};

        }
        self.avg_volt = self.tot_volt()%12;

    }

    fn get_next_cell_volt(&mut self) -> u16 {
        let volt: u16 = self.cell_volts[self.index_cell];
        self.index_cell += 1;
        volt
    }

    pub fn avg_volt(&self) -> u16 {
        self.avg_volt
    }

    pub fn tot_volt(&self) -> u16 {
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
}