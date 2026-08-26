
# STM32H563VI Porting Report

## Summary

The `bmslv` firmware now targets the STM32H563VI (`stm32h563vi`) with Embassy STM32 0.6, pinned to revision `a2b2a3e190a244df16a9a8ec03d7fb9d91d8297c` for reproducible builds. The application task structure, LTC6811 update and balancing state machine, CAN message encoding, fault timing, current calculation, and USB logging behavior remain intact.

Hardware configuration was reconciled against:

- the supplied KiCad STM32H563VI pinout;
- `smu/lib/drivers/instances/stm32h5xx/opaque/opaque_common.cpp` for RCC/PWR/FLASH and peripheral clocks;
- `smu/config/{front,rear}/driver_maps/instances/stm32h5xx/mapping.hpp` for the STM32H5 FDCAN configuration;
- Embassy's generated STM32H563VI peripheral metadata for pin alternate functions, ADC channels, and DMA requests.

## Pinout mapping

| KiCad signal | MCU pin | STM32 function | Rust use | Location |
|---|---:|---|---|---|
| `I_sense` | PA1 | ADC1_IN1, analog | Current sensing | `src/main.rs` |
| `Post_fuse_voltage` | PA2 | Analog-capable GPIO | Reserved by schematic; not consumed by current application logic | — |
| `CS` | PA4 | GPIO output | LTC6811 active-low chip select | `src/main.rs`, `src/ltc_management/spi_device.rs` |
| `SCK` | PA5 | SPI1_SCK, AF5 | LTC6811 SPI clock | `src/main.rs`, `src/ltc_management/spi_device.rs` |
| `MISO` | PA6 | SPI1_MISO, AF5 | LTC6811 SPI receive | `src/main.rs`, `src/ltc_management/spi_device.rs` |
| `MOSI` | PA7 | SPI1_MOSI, AF5 | LTC6811 SPI transmit | `src/main.rs`, `src/ltc_management/spi_device.rs` |
| `usb-` | PA11 | USB_DM, AF10 | USB CDC/defmt | `src/usb_serial/usb.rs` |
| `usb+` | PA12 | USB_DP, AF10 | USB CDC/defmt | `src/usb_serial/usb.rs` |
| `CAN_RXD` | PD0 | FDCAN1_RX, AF9 | CAN receive | `src/can_management/can_controller.rs` |
| `CAN_TXD` | PD1 | FDCAN1_TX, AF9 | CAN transmit | `src/can_management/can_controller.rs` |
| `TEMPERATURE` | PB12 | GPIO output | Temperature fault LED | `src/main.rs` |
| `VOLTAGE` | PB13 | GPIO output | Voltage fault LED | `src/main.rs` |
| Status LED output | PB14 | GPIO output | Debug/fault status LED | `src/main.rs` |
| `DEnable` | PB15 | GPIO output | Fault/error enable output | `src/main.rs` |
| `TMS/SWDIO` | PA13 | AF0 debug | SWD programming/debugging, left to reset/debug configuration | — |
| `TCK/SWCLK` | PA14 | AF0 debug | SWD programming/debugging, left to reset/debug configuration | — |
| HSE crystal | PH0/PH1 | OSC_IN/OSC_OUT | 12 MHz external crystal | `src/usb_serial/mod.rs` |

Embassy selects the alternate functions through its typed pin traits. The STM32H563VI metadata confirms ADC1 channel 1 on PA1, FDCAN1 AF9 on PD0/PD1, and USB AF10 on PA11/PA12. PA4 and PB12–PB15 are ordinary push-pull outputs and therefore do not use an alternate function.

No application timer output or input is routed to a physical pin in the supplied schematic. Embassy's internal time driver supplies executor timers. SPI DMA routing is internal rather than a pin alternate function:

| DMA channel | Request | Use |
|---|---|---|
| GPDMA1 channel 0 | SPI1_TX request 7 | SPI1 transmit |
| GPDMA1 channel 1 | SPI1_RX request 6 | SPI1 receive |

## Clock tree

The Rust RCC configuration mirrors the SMU clock setup:

| Clock | Configuration | Result |
|---|---|---:|
| HSE | Crystal oscillator | 12 MHz |
| PLL1 input | HSE / 3 | 4 MHz |
| PLL1 VCO | 4 MHz × 125 | 500 MHz |
| PLL1 P/Q/R | /2, /2, /2 | 250 MHz each |
| SYSCLK | PLL1 P | 250 MHz |
| HCLK | SYSCLK / 1 | 250 MHz |
| PCLK1 | HCLK / 1 | 250 MHz |
| PCLK2 | HCLK / 1 | 250 MHz |
| PCLK3 | HCLK / 1 | 250 MHz |
| PLL2 input | HSE / 2 | 6 MHz |
| PLL2 VCO | 6 MHz × 60 | 360 MHz |
| FDCAN kernel | PLL2 Q (/9) | 40 MHz |
| ADC/DAC kernel | PLL2 R (/6) | 60 MHz |
| USB kernel | HSI48 with USB synchronization | 48 MHz |
| LPTIM2 kernel | PCLK1 | 250 MHz |

Voltage scale 0 is selected. Embassy programs the required flash wait states while switching clocks; this corresponds to the SMU HAL's explicit `FLASH_LATENCY_5`. `Hsi48Config { sync_from_usb: true }` enables the HSI48/USB synchronization behavior corresponding to SMU's CRS setup. Embassy peripheral constructors perform the RCC enables and resets for ADC1, FDCAN1, SPI1, GPDMA1, GPIO banks, and USB. USB initialization also enables the H5 USB supply path through the Embassy USB backend.

ADC1 uses the SMU peripheral clock selection and acquisition settings: PLL2_R at 60 MHz, asynchronous divide-by-4, 12-bit conversion, and a 92.5-cycle sample time. The application retains polling reads because changing current sensing to a DMA window would alter its original averaging/update behavior.

FDCAN1 uses the SMU 40 MHz kernel clock and a 1 Mbit/s nominal rate. Embassy calculates the nominal bit timing from the selected kernel clock. Operation remains Classic CAN (up to eight payload bytes), matching the existing protocol and SMU configuration; the hardware remains CAN-FD capable.

## Drivers and integration points

| Driver/integration | Location | Implementation |
|---|---|---|
| STM32H563VI startup, task wiring, ADC and GPIO | `src/main.rs` | Embassy executor, ADC, GPIO, GPDMA interrupt bindings |
| RCC/PWR/peripheral clock tree | `src/usb_serial/mod.rs` | Embassy STM32 RCC configuration translated from SMU |
| FDCAN1 controller | `src/can_management/can_controller.rs` | Embassy FDCAN configurator and async transmit/receive |
| CAN frame adapter | `src/can_management/frame.rs` | Application frame type to Embassy classic CAN frame conversion |
| SPI1/GPDMA transport | `src/ltc_management/spi_device.rs` | Embassy async SPI master with GPDMA1 CH0/CH1 |
| LTC6811 driver/state machine | `src/ltc_management/ltc6811.rs` | Existing logic retained on the new SPI transport |
| USB CDC serial | `src/usb_serial/usb.rs` | Embassy STM32 USB FS and Embassy USB CDC ACM |
| USB defmt logger | `src/usb_serial/log.rs` | Existing defmt framing over CDC queue |
| Memory/link integration | `memory.x`, `build.rs` | 2 MiB flash, 640 KiB SRAM, aligned text placement |

## Logic changes required by STM32H563VI

- Replaced STM32F4/H7-style peripheral names and DMA channels with STM32H5 FDCAN1, USB, and GPDMA1 resources.
- Moved CAN to KiCad pins PD0/PD1, avoiding the PA11/PA12 USB pair.
- Moved fault/status outputs from provisional PC/PA pins to KiCad pins PB12–PB15.
- Updated ADC ownership and sample-time calls for Embassy's H5 ADC API.
- Updated Embassy task spawning for the current executor API.
- Added a 50 microsecond receive timeout so the polling-style CAN receive task releases the shared controller mutex when the bus is idle. This preserves the original nonblocking behavior and prevents CAN transmission starvation.
- Updated SPI storage to Embassy's async master communication mode and added the required GPDMA interrupt bindings.

## HAL and Embassy gaps

No PAC-only peripheral driver was required. Embassy STM32 currently provides the necessary STM32H563VI ADC, FDCAN, SPI/GPDMA, GPIO, RCC, time-driver, and USB Device FS support.

The relevant integration differences were API-level rather than missing hardware support:

- Embassy configures GPIO alternate functions from typed pin metadata rather than explicit AF numbers in application code.
- Embassy calculates FDCAN bit timing from the 40 MHz kernel clock instead of accepting the SMU HAL timing structure directly.
- Embassy automatically derives flash latency from the selected voltage scale and clock frequency.
- SPI DMA request selection is generated from STM32H563VI metadata and bound to GPDMA interrupts at compile time.

## Verification

The following checks pass for `thumbv7em-none-eabihf`:

```text
cargo fmt --all -- --check
cargo build
cargo build --release
git diff --check
```

Both development and release builds complete without compiler or linker warnings.

## Debug Report

### LTC-independent diagnostic firmware

- The default build now excludes ADC, FDCAN, USB, SPI1, and LTC6811 construction, while preserving the complete hardware path behind the `ltc-hardware` Cargo feature.
- This stricter isolation followed a hardware test in which excluding only LTC6811 did not produce an executor heartbeat, proving the observed stall was not uniquely attributable to LTC6811.
- PB14 is driven by an independent 250 ms heartbeat task. A steady PB14 after reset means the executor is not scheduling the heartbeat; regular blinking proves task scheduling without relying on the LTC6811.
- PB15 (`DEnable`) remains high for the complete isolated run. PB12 and PB13 are held low.
- Restore the full LTC path with `cargo run --features ltc-hardware` after isolation testing.
- The GPIO/executor-only hardware test passed: PB14 blinked, PB12/PB13 remained low, and PB15 remained high. This verifies firmware boot, Embassy RCC initialization, executor scheduling, the time driver, and GPIO operation. The PB13 illumination during DFU was confirmed as hardware reset-state behavior.
- The next default diagnostic layer enables only FDCAN1 in addition to the proven GPIO heartbeat. It transmits standard identifier `0x123` with payload `48 35 63 61 6e` every 200 ms; ADC, USB, SPI, and LTC remain excluded.
- The isolated FDCAN test passed on hardware and produced SocketCAN traffic, validating the 40 MHz kernel clock, 1 Mbit/s bit timing, PD0/PD1 routing, interrupts, transmit path, and PB15 enable state.
- The fixed diagnostic frame was then replaced by the original application `send_can` and `read_can` tasks over a zero-initialized BMS state. This tests application CAN scheduling and mutex interaction independently of ADC, USB, SPI, and LTC.
- With both application CAN tasks active, hardware traffic stopped while the independent GPIO heartbeat continued. The next isolation image retains the original publisher but removes `read_can`. That task repeatedly cancelled an interrupt-driven receive future after 50 us while sharing the same `Can` object and mutex with TX; validation is pending before refactoring TX and RX ownership.
- TX-only application publishing passed on hardware. FDCAN ownership is now split into Embassy `CanTx` and `CanRx` halves. TX alone retains its mutex for the publisher and fault sender; RX is owned directly by `read_can` and remains continuously pending until a frame or bus error arrives. The unsafe 50 us receive cancellation loop and TX/RX mutex contention were removed.

### SPI mode and CAN driver-enable isolation

- Restored SPI1 mode 3, matching the project's last known Rust LTC6811 driver. The earlier mode-0 substitution came from a generic C SPI mapping and produced no PEC-valid LTC groups on hardware.
- PB15 (`DEnable`) now starts high so the external driver path is enabled independently of LTC6811 initialization. Existing fault handling retains control of the pin after startup.
- Hardware validation is pending a manual flash, BOOT/reset, LED observation, and SocketCAN capture.

### CAN

- Confirmed the FDCAN kernel source is PLL2_Q at 40 MHz.
- Replaced Embassy's generic 1 Mbit/s calculation with the SMU timing: prescaler 2, segment 1 = 15 TQ, segment 2 = 4 TQ, SJW = 4 TQ. This gives 20 TQ per bit, a 1 Mbit/s nominal rate, and an 80% sample point.
- Applied the same timing to the data timing register for parity with the SMU configuration. The application continues to transmit Classic CAN frames because its protocol payloads are at most eight bytes and SMU selects `FDCAN_FRAME_CLASSIC`.
- Confirmed automatic retransmission is enabled, transmit pause is disabled, protocol exception handling is disabled, and transmit FIFO mode is selected.
- Confirmed the default global filter accepts unmatched standard and extended frames into RX FIFO0. FDCAN1 IT0 and IT1 are bound to Embassy handlers; IT0 services RX, TX completion, and bus-off recovery.
- `can0` is up at 1 Mbit/s, ERROR-ACTIVE, with zero warning, passive, bus-off, arbitration-loss, or bus-error counters.
- `cargo run --locked` successfully erased and downloaded 102,464 bytes through STM32 ROM DFU at `0x08000000`.
- The first post-flash capture contained no frames because the STM32 remained in ROM DFU mode (`0483:df11`). Physical BOOT-button activation is required before the firmware and CAN test can run. A successful post-BOOT `candump can0` capture is still pending and is not claimed here.
- A later hardware cycle reported no post-BOOT CAN frames. CAN transmit completion is now bounded to 10 ms so missing acknowledgements or a stuck completion interrupt cannot permanently stall the shared CAN update path. Hardware validation of this change is pending.
- Custom-driver review found that Embassy logs the RCC frequencies during `embassy_stm32::init()`, before the custom USB serial queues are initialized. The global defmt logger dereferenced a null TX queue pointer at that point, potentially HardFaulting before CAN and GPIO startup. USB serial queue access now safely drops pre-initialization log records. Hardware validation is pending.
- Hardware subsequently showed the voltage fault output active, proving execution reached the fault task, but CAN remained silent. The manually translated SMU bit timing was replaced with Embassy's native bitrate calculation. With the 40 MHz kernel clock this yields prescaler 4, segment 1 = 8, segment 2 = 1, and SJW = 1, matching the STM32H563VI `bmslv_cpp` configuration. Hardware validation is pending.

### Fault LEDs

- Voltage fault output PB13 now explicitly clears whenever voltage returns to the valid range. Its debounce timer is reset only while readings are valid, so an old fault cannot remain latched or immediately re-trigger without another sustained invalid reading.
- Temperature fault output PB12 asserts after the existing 450 ms debounce when a temperature is missing or invalid.
- Debug fault output PB14 now toggles during any active voltage or temperature fault. The previous code incorrectly toggled PB12, conflating the debug blink with the temperature indication.
- Voltage and temperature outputs initialize low. PB14 is handled by the staged boot marker described below.
- PB14 now also acts as a staged boot marker. A minimal PAC routine asserts it before `embassy_stm32::init()`. Embassy then keeps it solid high throughout peripheral and LTC initialization, clears it immediately before spawning the LTC update task, and subsequently uses it for the normal active-fault blink. Solid high therefore indicates a stall before task startup; blinking indicates the fault task is running; low indicates task startup completed with no active fault. Hardware validation is pending.
- Hardware showed PB13 transitioning low while PB14 remained solid high after reset, localizing the stall to LTC6811 initialization after RCC/GPIO setup. LTC initialization was moved out of `main` and into the LTC task so a stalled SPI/LTC device cannot prevent the executor, CAN RX, and remaining tasks from starting. PB14 now clears only after LTC initialization returns. Hardware validation is pending.
- Hardware safety defaults are now asserted immediately: PB12 and PB13 start high, while PB15 (`DEnable`) starts low. They cannot briefly indicate a healthy system before sensor validation.
- PB14 follows the required polarity: it stays high when no fault is active and toggles when a debounced voltage or temperature fault is active.
- LTC6811 initialization and each update are bounded to 500 ms. A missing or non-responsive LTC records a failed update but cannot block CAN or prevent the fail-safe LED/enable state from being evaluated.

### Temperature pipeline

- Unified the application and LTC6811 thermistor counts at 12. The previous four-element BMS temperature array caused an out-of-bounds task panic when the LTC loop reached thermistor index 4.
- Corrected temperature thresholds to the stored tenths-of-a-degree representation: 10.0 °C minimum is 100 and 60.0 °C maximum is 600.
- Missing samples remain zero and therefore assert the temperature fault. A zero LTC AUX conversion maps to an invalid high value and also asserts the fault.
- The existing mux selection, AUX conversion, PEC check, parsing, history update, and CAN temperature-frame sequence remain structurally unchanged.
- Corrected technical CAN accessors to read the most recently completed history slot rather than the newly advanced empty slot. Valid LTC readings therefore no longer appear as default zeros after `SLAVEBMS::update()`.
- Each LTC cycle now clears and rebuilds 12-bit cell and thermistor sample masks. Fault evaluation requires a complete mask, so default values and partial/timed-out cycles cannot be treated as measurements.
- Cell and AUX PEC mismatches are reported through defmt but do not suppress raw diagnostic values. Strict rejection was reverted after hardware testing showed it hid the previously visible maximum cell ADC readings and left all diagnostic masks at zero. PEC enforcement remains pending correction of the LTC transaction/alignment path.
- CAN temperature frame `0x220` now sets the DBC-defined `fault_temp_lv` signal at bit 30 (byte 3, bit 6) from the temperature fault state.
- CAN error frame `0x05A` retains its original contract: byte 0 bit 0 is one when either the voltage or temperature fault is active.
- New diagnostic frame `0x05B` reports the current LTC acquisition state: bytes 0-1 are the 12-cell sample mask, bytes 2-3 are the 12-temperature sample mask, and bytes 4-7 are the little-endian count of fully completed LTC measurement cycles. The existing `0x2F8-0x2FD` frames carry all raw cell and temperature values.
- Diagnostic masks are cumulative since boot, so a valid LTC group remains observable even if a later conversion times out. Technical frames expose samples already accepted in the current partial cycle instead of hiding them until the entire voltage-plus-temperature cycle completes.
- Replaced the placeholder mux encoding with the LTC1380 SMBus send-byte protocol. U1 uses strapped write address `0x90`, U2 uses `0x94`, and each command sends `EN=1` plus the three-bit channel.
- Corrected LTC6811 `WRCOMM` to `0x0721` and `STCOMM` to `0x0723`, appended the required COMM data PEC, and supplies 72 clock pulses after STCOMM while CS remains low.
- GPIO4/SDA and GPIO5/SCL are both released high in CFGR0 as required for the LTC6811 open-drain I2C master.

### ADC

- Verified current sense is PA1/ADC1_IN1 from STM32H563VI metadata and the supplied KiCad pinout.
- ADC/DAC kernel clock is PLL2_R at 60 MHz, followed by the SMU asynchronous divide-by-4 setting.
- ADC resolution is explicitly programmed to 12 bit instead of relying on the peripheral reset state; sample time remains 92.5 cycles, matching the SMU configuration.
- Current sensing retains its original polling and software averaging loop. GPDMA is not used for this single channel because converting it to DMA would change the update behavior; GPDMA1 channels 0 and 1 remain dedicated to SPI1 TX/RX.
- Replaced the obsolete LEM conversion with the fitted ACS773LCB-100B transfer function. At 3.3 V its nominal sensitivity is 13.2 mV/A and its zero-current output is VCC/2. Conversion is performed ratiometrically as ADC-count delta divided by 16.38 counts/A, preserving the existing internal 0.1 mA and CAN 0.01 A units.
- PB15 remains low during a 2-second analog settling interval and a 512-sample zero-current calibration. The fault task may assert PB15 only after calibration completes and all voltage/temperature faults are clear, preventing the enabled 48 V/LTC load from biasing the baseline.
- Diagnostic frame `0x05C` contains the ACS773 offset ADC count, live averaged ADC count, signed delta in Q8 count units, and signed current in 0.01 A units. This exposes sub-count motion hidden by the production signal's quantization.

### State of charge

- Added a compact voltage-to-SOC lookup table for the 12s3p Molicel INR-21700-P50B pack, with linear interpolation from 30.0 V/0% to 50.4 V/100%.
- CAN frame `0x21E` retains signed current in bytes 0-1 and now carries estimated remaining energy in bytes 2-3 using the DBC scale of 0.1 Wh per bit. Full-scale energy is 648 Wh; the field is zero until all LTC cell register groups pass PEC validation.

### Cleanup

- Removed runtime defmt logging, the USB defmt logger, unused SPI methods, an unused BMS constructor, an unused temperature counter, and unused direct dependencies.
- Retained diagnostic CAN frames `0x05B`, `0x05C`, and `0x05D`.
- Corrected the compilation target from Cortex-M7 to the STM32H563 Cortex-M33 target `thumbv8m.main-none-eabihf`.
- Retained `memory.x` because `build.rs` installs it in the linker search path and `link.x` uses it to place flash, RAM, the vector table, and program text.
- Diagnostic frame `0x05D` contains LTC configuration/cell/AUX PEC validity, latest cell group, received and calculated cell PEC values, and wrapping cell/AUX PEC error counters.

### Peripheral timing

- SYSCLK, HCLK, PCLK1, PCLK2, and PCLK3 are 250 MHz.
- FDCAN1 kernel clock is 40 MHz from PLL2_Q.
- ADC kernel clock is 60 MHz from PLL2_R and ADC conversion clock is 15 MHz after divide-by-4.
- SPI1 kernel clock is PLL1_Q at 250 MHz; Embassy selects a hardware divider for the configured 1 MHz SPI bus.
- USB uses synchronized HSI48 at 48 MHz.
- LPTIM2 uses PCLK1 at 250 MHz; Embassy owns the active executor time-driver timer and its interrupt configuration.
- GPDMA request 7 drives SPI1 TX on channel 0 and request 6 drives SPI1 RX on channel 1.
