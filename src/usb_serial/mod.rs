#[cfg(feature = "ltc-hardware")]
pub mod usb;

#[cfg(feature = "ltc-hardware")]
pub use usb::Serial;

use embassy_stm32::time::Hertz;
use embassy_stm32::Config;

pub fn prepare_config() -> Config {
    let mut config = Config::default();

    {
        use embassy_stm32::rcc::*;

        config.rcc.hse = Some(Hse {
            freq: Hertz(12_000_000),
            mode: HseMode::Oscillator,
        });

        config.rcc.pll1 = Some(Pll {
            source: PllSource::Hse,
            prediv: PllPreDiv::Div3,
            mul: PllMul::Mul125,
            divp: Some(PllDiv::Div2),
            divq: Some(PllDiv::Div2),
            divr: Some(PllDiv::Div2),
        });

        config.rcc.pll2 = Some(Pll {
            source: PllSource::Hse,
            prediv: PllPreDiv::Div2,
            mul: PllMul::Mul60,
            divp: None,
            divq: Some(PllDiv::Div9),
            divr: Some(PllDiv::Div6),
        });

        config.rcc.ahb_pre = AHBPrescaler::Div1;
        config.rcc.apb1_pre = APBPrescaler::Div1;
        config.rcc.apb2_pre = APBPrescaler::Div1;
        config.rcc.apb3_pre = APBPrescaler::Div1;
        config.rcc.voltage_scale = VoltageScale::Scale0;

        config.rcc.sys = Sysclk::Pll1P;

        config.rcc.hsi48 = Some(Hsi48Config {
            sync_from_usb: true,
        });
        config.rcc.mux.usbsel = mux::Usbsel::Hsi48;
        config.rcc.mux.fdcan12sel = mux::Fdcansel::Pll2Q;
        config.rcc.mux.adcdacsel = mux::Adcdacsel::Pll2R;
        config.rcc.mux.lptim2sel = mux::Lptim2sel::Pclk1;
    }

    config
}
