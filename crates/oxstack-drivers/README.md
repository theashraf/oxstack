# oxstack-drivers

Async, allocation-free `no_std` drivers for the chips on M5Stack devices,
written against the `embedded-hal` / `embedded-hal-async` 1.0 traits only,
so they run on any HAL and are unit-tested on the host.

| Chip | Feature | What it is |
|---|---|---|
| TI TCA8418 | `tca8418` | I²C keypad scanner with a hardware event FIFO |
| Sitronix ST7789 | `st7789` | SPI TFT controller (RGB565), zero-copy pixel streaming |
| Bosch BMI270 | `bmi270` | 6-axis IMU; links Bosch's 8 KB config blob into flash |
| Everest ES8311 | `es8311` | mono audio codec, I²S slave, silent until playback is requested |

All four are on by default; disable default features to compile only what
you use. `defmt` adds `defmt::Format` to the public types.

```rust
use oxstack_drivers::tca8418::{Event, KeyEvent};

// Raw FIFO byte 0x81: key at ROW0/COL0 pressed.
assert_eq!(
    Event::decode(0x81),
    Some(Event::Key(KeyEvent { row: 0, col: 0, pressed: true })),
);
```

## Licensing

MIT, except `src/bmi270/config.bin`, which is Bosch Sensortec's
configuration file under BSD-3-Clause (`src/bmi270/LICENSE-BOSCH`).
Register values adapted from M5GFX, M5Unified and Espressif's esp-bsp are
credited in `NOTICE`.

Part of [oxstack](https://github.com/theashraf/oxstack).
