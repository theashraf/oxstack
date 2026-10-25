# Changelog

## 0.2.0 — 2026-10-25

Breaking release of `oxstack-cardputer`, driven by feedback from building a
launcher on 0.1: every piece of hardware could be initialised only once per
boot, the SD card was writable by default, and the IMU's axes were a guess.

### Hardware can be used more than once per boot

Every resource group's `init` now **borrows** the group (`&mut self`) and
returns a driver with a lifetime, instead of consuming `'static`
peripherals. Drop the driver and the group is free again: a menu can run the
same app twice, or the microphone after the speaker. Groups also gained
`reborrow()`, like `esp-hal` peripherals, so they can be lent to functions.

| 0.1 | 0.2 |
|---|---|
| `board.display.init()` → `Display` | `board.display.init()` → `Display<'_>` (same call; take `board` as `mut`) |
| `board.audio.into_recorder(..)` | `board.audio.recorder(..)` → `Recorder<'_>` |
| `board.audio.into_player(..)` | `board.audio.player(..)` → `Player<'_>` |
| `board.spi_bus.into_sd()` | `board.spi_bus.sd()` → `Sd<'_>`, **read-only by default** |
| `Rmt::new(board.system.rmt, ..)` + `RgbLed::new` + `IrTx::new` | `board.rmt.init()` → `(RgbLed<'_>, IrTx<'_>)` |
| `board.rgb_led`, `board.ir_tx`, `board.system.rmt` | folded into `board.rmt` |
| `imu::Imu` = `Bmi270<..>` | `Imu<'_>` wrapper; `read()` is in the board frame, `read_chip()` is not |
| `Keyboard`, `Backlight`, `Battery`, `RgbLed`, `IrTx`, `Sd`, `Recorder`, `Player` | all carry a `'d` lifetime |

`SysI2cResources::into_bus`, `display::framebuffer` and `net::start_station`
stay one-per-boot by nature; `i2c::bus()` returns the bus from anywhere.

Only one DMA stream buffer is kept for audio (recorder or player, never
both), saving 32 KB of RAM compared with 0.1.

### SD card is read-only by default

`embedded-sdmmc` writes FAT bookkeeping even for a directory listing.
`Sd` now refuses writes with `sd::Error::ReadOnly` (and a `defmt` warning)
until `set_writable(true)`. `sd::Error` is a new enum wrapping
`SdCardError`.

### IMU axes

`imu` documents the board frame (+X right, +Y toward the top edge, +Z out of
the screen) and applies `imu::BOARD_AXES`, an `AxisMap` from the new
`oxstack_drivers::axes` module. Only Z is confirmed so far; the new
`imu_axes` example measures the mapping in three poses and prints the
constant. `AxisMap::from_gravity` is the host-tested core of that tool.

### New helpers

- `net::scanner(wifi)` / `Scanner::scan()`: Wi-Fi scan with a per-channel
  dwell that actually finds networks, results strongest first, radio
  released on drop.
- `ble::stack(..)` and `ble::scan(&stack, duration, |adv| ..)`: scanning
  without the trouble-host glue; `Advertisement::name()` parses names.
- `WallClock::unix_micros`, `subsec_micros`, `next_second` for redrawing a
  clock exactly on the second.
- `oxstack_gfx::TextBuf<N>` and `text!(N, ..)`: a fixed-capacity formatting
  buffer that never fails or panics.
- `Recorder::shutdown` / `Player::shutdown` power the codec down before the
  stream stops; `Player::write_some` is the cancel-safe way to feed audio
  from a `select`. Cancel safety is now documented for every `async` driver
  method.
- `oxstack_cardputer` re-exports `esp_hal`, `esp_rtos`, `embassy_executor`,
  `embassy_time`, `embassy_sync`, `embassy_futures`,
  `esp_bootloader_esp_idf` and `oxstack_drivers` (as `drivers`).

### Getting started

- `templates/cardputer-app`: a `cargo generate` template with the pinned
  versions, `.cargo/config.toml`, `build.rs` and a minimal `main.rs`.
- The crate docs describe the project setup, and CI publishes the API docs
  for the ESP32-S3 target to GitHub Pages (docs.rs cannot build Xtensa).

### Examples

`menu` runs five apps repeatedly without rebooting; `imu_axes` finds the IMU
mounting. All examples use `TextBuf`.

## 0.1.0 — 2026-10-25

Initial release: display, backlight, keyboard, speaker and microphone,
BMI270, battery, microSD, RGB LED, IR, Wi-Fi and BLE on the Cardputer ADV.
