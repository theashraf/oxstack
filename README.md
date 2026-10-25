# oxstack

Pure-Rust, `no_std`, async support for M5Stack hardware, starting with the
**M5Stack Cardputer ADV**. Built on `esp-hal` and Embassy, with no ESP-IDF
and no C++.

> **Status: hardware bring-up complete.** Every on-board device runs on a
> real Cardputer ADV: display, backlight, keyboard (56/56 keys), speaker
> and microphone, BMI270 IMU, battery sensing, microSD (read), RGB LED, G0
> button, IR LED, Wi-Fi (scan, connect, DHCP, SNTP clock) and Bluetooth LE
> (scan, GATT peripheral).

```rust
use oxstack_cardputer::input::Action;

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let mut board = oxstack_cardputer::init(oxstack_cardputer::default_config());
    let _backlight = board.backlight.init();
    let i2c = board.sys_i2c.into_bus();
    let mut lcd = board.display.init().await.unwrap();
    let mut keyboard = board.keyboard.init(i2c).await.unwrap();
    let fb = oxstack_cardputer::display::framebuffer();

    loop {
        let ev = keyboard.next_event().await.unwrap();
        if let Some(Action::Char(c)) = ev.action() { /* draw c into fb */ }
        lcd.flush(fb).await.unwrap(); // sends only the rows that changed
    }
}
```

Every driver **borrows** its hardware from `board` and gives it back when
dropped, so a menu can run the same app twice, or the microphone after the
speaker, without rebooting. See the [`menu`](examples/cardputer/src/bin/menu.rs)
example.

## Crates

| Crate | What | Builds on |
| --- | --- | --- |
| [`oxstack-cardputer`](crates/oxstack-cardputer) | Board support: pins, drivers, Wi-Fi/BLE helpers | ESP32-S3 only |
| [`oxstack-drivers`](crates/oxstack-drivers) | ST7789, TCA8418, ES8311, BMI270 over `embedded-hal` | host + target |
| [`oxstack-gfx`](crates/oxstack-gfx) | Dirty-row RGB565 framebuffer, `TextBuf` | host + target |
| [`oxstack-input`](crates/oxstack-input) | Key matrix, modifiers, `Action`s | host + target |
| [`oxstack-audio`](crates/oxstack-audio) | Tone generator, level meter | host + target |
| [`oxstack-net`](crates/oxstack-net) | SNTP, calendar time | host + target |
| [`oxstack-ir`](crates/oxstack-ir) | IR protocol encoding (NEC) | host + target |

API docs for the board crate are built for the ESP32-S3 and published at
**<https://theashraf.github.io/oxstack/oxstack_cardputer/>** (docs.rs cannot
build for Xtensa). The portable crates are on docs.rs as usual.

## Start a project

```sh
cargo install cargo-generate espup espflash --locked
espup install                      # Xtensa Rust toolchain → ~/export-esp.sh
source ~/export-esp.sh             # every new shell
cargo generate --git https://github.com/theashraf/oxstack templates/cardputer-app
cd <your-project> && cargo run --release   # build, flash, defmt monitor
```

The template holds everything a firmware crate needs and that is easy to
get wrong: the pinned `esp-hal` / `esp-rtos` / Embassy versions, the
`.cargo/config.toml` (target, `build-std`, `-nostartfiles`, `espflash`
runner), the `build.rs` linker scripts (`-Tdefmt.x`, then `-Tlinkall.x`),
the `esp` toolchain file, and a minimal `main.rs`. The same checklist is in
the [crate docs](crates/oxstack-cardputer/src/lib.rs).

## Work on oxstack itself

```sh
cargo test                         # portable crates, on your machine
cd examples/cardputer
cargo run --release --bin i2c_scan # flash + defmt log monitor
cargo run --release --bin display  # alignment test + flush benchmark
cargo run --release --bin menu     # five apps, re-run without rebooting
cargo run --release --bin imu_axes # measure how the IMU is mounted
```

From the repo root:

| Command              | What it does                                              |
| -------------------- | --------------------------------------------------------- |
| `cargo test`         | Host tests for drivers, graphics, input, audio, net, ir   |
| `cargo check-target` | Type-check the ESP32-S3 crates                            |
| `cargo xtask size`   | Build all firmware; report flash image / `.data` / `.bss` |

### Wi-Fi examples

Copy `examples/cardputer/wifi.toml.example` to `examples/cardputer/wifi.toml`
(git-ignored) and fill in your network, then:

```sh
cd examples/cardputer
cargo run --release --bin wifi_connect --features wifi   # DHCP + SNTP clock
cargo run --release --bin wifi_scan --features wifi
```

### Bluetooth examples

```sh
cd examples/cardputer
cargo run --release --bin ble_scan --features ble
cargo run --release --bin ble_peripheral --features ble
uv run --with bleak ../../tools/ble_check.py   # drive the peripheral from a computer
```

## License

MIT ([`LICENSE`](LICENSE)). `oxstack-drivers` also ships Bosch's BMI270
configuration file under BSD-3-Clause. Values adapted from M5GFX, M5Unified
and Espressif's esp-bsp are credited in [`NOTICE`](NOTICE).
