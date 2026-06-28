//! USB-CDC serial and clock configuration.
//!
//! Exposes [`prepare_config`], which builds the STM32F405 clock tree (12 MHz
//! HSE -> 168 MHz system clock, 48 MHz for USB), and re-exports the USB serial
//! ([`usb`]) and defmt logger ([`log`]) submodules.

pub mod usb;
pub mod log;

use embassy_stm32::Config;
use embassy_stm32::time::Hertz;

/// Builds the clock configuration for the board.
///
/// Uses the 12 MHz external oscillator through the main PLL to reach a 168 MHz
/// system clock, and routes PLL-Q to the 48 MHz USB clock. APB1/APB2
/// prescalers follow the F4 maximum-frequency constraints.
pub fn prepare_config() -> Config {
    let mut config = Config::default();
    {
        use embassy_stm32::rcc::*;
        // External 12 MHz crystal.
        config.rcc.hse = Some(Hse {
            freq: Hertz(12_000_000),
            mode: HseMode::Oscillator,
        });

        // PLL: 12 MHz / 6 * 168 / 2 = 168 MHz system clock; /7 gives 48 MHz.
        config.rcc.pll_src = PllSource::HSE;
        config.rcc.pll = Some(Pll {
            prediv: PllPreDiv::DIV6,
            mul: PllMul::MUL168,
            divp: Some(PllPDiv::DIV2),
            divq: Some(PllQDiv::DIV7),
            divr: None,
        });

        // Bus prescalers (APB1 <= 42 MHz, APB2 <= 84 MHz).
        config.rcc.ahb_pre = AHBPrescaler::DIV1;
        config.rcc.apb1_pre = APBPrescaler::DIV4;
        config.rcc.apb2_pre = APBPrescaler::DIV2;

        config.rcc.sys = Sysclk::PLL1_P;
        config.rcc.mux.clk48sel = mux::Clk48sel::PLL1_Q;
    }
    config
}
