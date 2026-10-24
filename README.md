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
    let board = oxstack_cardputer::init(oxstack_cardputer::default_config());
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

## Getting started

```sh
cargo install espup espflash --locked
espup install                      # Xtensa Rust toolchain → ~/export-esp.sh
source ~/export-esp.sh             # every new shell

cargo test                         # portable crates, on your machine
cd examples/cardputer
cargo run --release --bin i2c_scan # flash + defmt log monitor
cargo run --release --bin display  # alignment test + flush benchmark
cargo run --release --bin typewriter
```

From the repo root:

| Command              | What it does                                              |
| -------------------- | --------------------------------------------------------- |
| `cargo test`         | Host tests for drivers, graphics, input                   |
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
