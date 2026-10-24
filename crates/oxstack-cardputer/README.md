# oxstack-cardputer

Board support for the **M5Stack Cardputer ADV** (ESP32-S3) on `esp-hal` and
Embassy, in pure Rust with no ESP-IDF. `Board` splits the chip's peripherals
into one group per board function (display, keyboard, audio, SD, IMU,
battery, LEDs, IR, radio), and each group's `init` returns a ready driver.

```rust,ignore
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
        // draw into fb, then send only the rows that changed:
        lcd.flush(fb).await.unwrap();
    }
}
```

| Feature | Adds |
|---|---|
| `wifi` | Station mode, DHCP, DNS, UDP/TCP via `embassy-net`, SNTP clock |
| `ble` | Bluetooth LE via the `trouble` host stack |
| `defmt` | `defmt::Format` on public types |

## Requirements

- The Xtensa Rust toolchain from [`espup`](https://github.com/esp-rs/espup).
- Target `xtensa-esp32s3-none-elf` with `-Zbuild-std=core,alloc`.
- `esp-hal` is pinned to `=1.2.2` and `esp-radio` to `=1.0.0-beta.1`,
  because their unstable APIs carry no semver guarantee.

docs.rs cannot build Xtensa targets; run `cargo doc` for this crate from
your firmware project. Firmware examples are in the
[repository](https://github.com/theashraf/oxstack/tree/main/examples/cardputer).

Part of [oxstack](https://github.com/theashraf/oxstack). Licensed under the MIT license.
