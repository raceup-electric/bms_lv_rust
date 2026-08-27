const PACK_ENERGY_DECI_WH: u32 = 6_480;

const SOC_TABLE: [(u32, u16); 11] = [
    (30_000, 0),
    (36_000, 20),
    (39_600, 80),
    (42_000, 180),
    (43_200, 320),
    (44_400, 500),
    (45_600, 650),
    (46_800, 780),
    (48_000, 880),
    (49_200, 950),
    (50_400, 1_000),
];

pub fn remaining_energy_deci_wh(total_voltage_100uv: u32) -> u16 {
    let pack_mv = total_voltage_100uv / 10;
    let soc_permille = interpolate_soc(pack_mv);
    ((PACK_ENERGY_DECI_WH * soc_permille as u32) / 1_000) as u16
}

pub fn soc_percentage(total_voltage_100uv: u32) -> u8 {
    let pack_mv = total_voltage_100uv / 10;
    ((interpolate_soc(pack_mv) + 5) / 10) as u8
}

fn interpolate_soc(pack_mv: u32) -> u16 {
    if pack_mv <= SOC_TABLE[0].0 {
        return SOC_TABLE[0].1;
    }

    for points in SOC_TABLE.windows(2) {
        let (low_mv, low_soc) = points[0];
        let (high_mv, high_soc) = points[1];
        if pack_mv <= high_mv {
            let voltage_offset = pack_mv - low_mv;
            let voltage_span = high_mv - low_mv;
            let soc_span = u32::from(high_soc - low_soc);
            return low_soc + ((voltage_offset * soc_span) / voltage_span) as u16;
        }
    }

    SOC_TABLE[SOC_TABLE.len() - 1].1
}
