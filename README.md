# BMS LV — Low-Voltage Battery Management System (Rust)

Firmware for the **Low-Voltage (LV) Battery Management System** of the RaceUP
Formula SAE car, rewritten **from scratch in Rust** on top of the
[Embassy](https://embassy.dev) async embedded framework.

The system monitors a 48 V LV pack (12 series cells), reads per-cell voltages and
pack temperatures from an **LTC6811** battery monitor over SPI, performs fault
detection and passive cell balancing, and reports state to the rest of the car
over CAN bus. Logging is exposed over USB-CDC and RTT (`defmt`).

> Developed as part of the RaceUP Electric team. Fork of
> [`raceup-electric/bms_lv_rust`](https://github.com/raceup-electric/bms_lv_rust).

---

## Highlights

- **`no_std` async firmware** built on Embassy (executor, time, sync, USB).
- **LTC6811 driver written from scratch** — cell-voltage and auxiliary
  (temperature) command groups implemented directly from the datasheet, driven
  over **SPI1 with DMA**.
- **Concurrent task architecture**: cell/temperature acquisition, current
  sensing, CAN TX, CAN RX and fault logic each run as independent Embassy tasks,
  sharing state through `Mutex`-guarded `StaticCell`s.
- **Safety logic**: over/under-voltage and over/under-temperature detection with
  time debouncing, a hardware error line, status LEDs and an error frame
  broadcast on CAN.
- **Passive balancing** through the LTC6811 discharge switches, commanded
  remotely over CAN.
- **Rolling-average filtering** over the last 5 acquisitions to smooth noisy
  measurements.

---

## Hardware

| Component        | Detail                                                        |
|------------------|---------------------------------------------------------------|
| MCU              | STM32F405RG (Cortex-M4F, `thumbv7em-none-eabihf`)             |
| Battery monitor  | Analog Devices **LTC6811** (12 cells + 4 thermistors / slave) |
| Pack             | 48 V nominal LV pack, 12 cells in series                      |
| Cell interface   | SPI1 (+ DMA2) to the LTC6811                                  |
| Vehicle bus      | CAN @ **500 kbps** (bxCAN)                                     |
| Current sensing  | ADC1 on a hall/shunt current input                            |
| Host link        | USB-CDC (USB OTG FS) for logs                                 |

### Pin mapping

| Function            | Pin(s)                          |
|---------------------|----------------------------------|
| SPI1 (LTC6811)      | SCK `PA5`, MISO `PA6`, MOSI `PA7`, CS `PA4` (DMA2 CH3/CH0) |
| CAN bus             | CAN2 `PB12`/`PB13` (CAN1 `PA11`/`PA12` enabled as master)  |
| Current sense (ADC) | `PA1` (ADC1)                     |
| USB-CDC             | USB OTG FS                       |
| Error / SDC line    | `PA2`                            |
| Status LEDs         | debug `PC13`, voltage `PC11`, temperature `PC9` |

> On the STM32F4, CAN2 is a slave instance that shares the filter bank with CAN1,
> so CAN1 is initialised even though the pack communicates on CAN2.

---

## Architecture

The firmware runs four cooperative Embassy tasks plus the main loop:

```
              +----------------------------------------------+
              |                main (init)                    |
              |  clocks, peripherals, shared StaticCell state |
              +----------------------------------------------+
                 |            |            |            |
        +--------+   +--------+   +--------+   +--------+
        v            v            v            v
 +-------------+ +----------+ +----------+ +--------------------+
 |current_sense| | send_can | | read_can | |   ltc_function     |
 | ADC1 -> I   | | V/T/tech | | balance/ | | LTC6811 read,      |
 | calibration | | frames   | | tech cmd | | faults, balancing, |
 |             | |          | |          | | error frame, log   |
 +-------------+ +----------+ +----------+ +--------------------+
        \____________ shared SLAVEBMS / CAN / LTC state ________/
```

Source layout:

```
src/
├── main.rs               # init + tasks (acquisition, CAN, faults, balancing)
├── types/
│   ├── bms.rs            # SLAVEBMS state, per-cell/temp data, rolling history
│   └── mod.rs            # CAN message IDs, voltage/temperature thresholds
├── ltc_management/
│   ├── ltc6811.rs        # LTC6811 command set + measurement/balancing logic
│   └── spi_device.rs     # SPI1 + DMA abstraction
├── can_management/
│   ├── can_controller.rs # bxCAN setup, read/write
│   └── frame.rs          # CAN frame helpers
└── usb_serial/           # USB-CDC + defmt logging
```

### CAN message map

| ID (hex) | Direction | Meaning                              |
|----------|-----------|--------------------------------------|
| `0x54`   | TX        | Cell voltages (max / min / avg / tot)|
| `0x55`   | TX        | Temperatures                         |
| `0x14`   | TX        | Error / fault state                  |
| `0x365`-`0x369` | TX | Extended "tech" telemetry            |
| `0x1A4`  | RX        | Balancing enable command             |
| `0x365`  | RX        | Tech-mode enable command             |

### Protection thresholds

| Parameter          | Value   |
|--------------------|---------|
| Cell over-voltage  | 4.20 V  |
| Cell under-voltage | 3.00 V  |
| Fault debounce     | ~450 ms |

Thermistors are NTC (22 kΩ, β = 3435 K), linearised with the Beta equation.

---

## Build & flash

The project targets `thumbv7em-none-eabihf` and pins a nightly toolchain via
`rust-toolchain.toml` (no manual setup needed — `rustup` picks it up).

```bash
# Build (release)
cargo build --release

# Build + flash over USB DFU (default cargo runner)
cargo run --release
```

Flashing uses the bundled `post_build.sh`, which converts the ELF to a raw
binary and writes it to flash:

```sh
arm-none-eabi-objcopy -O binary <elf> <elf>.bin
dfu-util -a 0 -s 0x08000000 -D <elf>.bin
```

Requirements: `arm-none-eabi-objcopy` (GNU Arm toolchain) and `dfu-util`.
To flash with a probe instead, switch the runner in `.cargo/config.toml` to
`probe-rs run --chip STM32F405RGTx`.

### Logs

`defmt` output is available over RTT (e.g. `probe-rs` / `defmt-print`) and a
human-readable stream is mirrored over USB-CDC.

---

## Tech stack

`Rust` · `Embassy` · `no_std` · `STM32F405` · `LTC6811` · `SPI + DMA` · `CAN` ·
`USB-CDC` · `defmt`

## Credits

RaceUP Electric — University of Padua, Formula SAE.
