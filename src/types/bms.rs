pub static NUM_CELLS: usize = 12;

#[derive(Default)]
pub struct BMS {
    slave_volts: [u16; NUM_CELLS],
    index_cell: usize,
    tot_volt: u32,
    max_volt: u16,
    min_volt: u16,
    avg_volt: u16,
    temp: u16,
}

impl BMS {
    pub async fn new() -> Self {
        BMS {
            slave_volts: [0; NUM_CELLS],
            max_volt: 0,
            min_volt: 0,
            avg_volt: 0,
            tot_volt: 0,
            temp: 0,
            index_cell: 0
        }
    }

    pub async fn get_next_slave_volt(&mut self) -> u16 {
        let volt: u16 = self.slave_volts[self.index_cell];
        self.index_cell += 1;
        volt
    }

    pub async fn avg_volt(&self) -> u16 {
        self.avg_volt
    }

    pub async fn tot_volt(&self) -> u32 {
        self.tot_volt
    }

    pub async fn min_volt(&self) -> u16 {
        self.min_volt
    }

    pub fn max_volt(&self) -> u16 {
        self.max_volt
    }

    pub async fn temp(&self) -> u16 {
        self.temp
    }
}