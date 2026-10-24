//! The WS2812-class RGB LED on GPIO21, driven by an RMT channel.
//!
//! **Powered through the LCD backlight pin:** it only works while the
//! backlight is fully on. See [`crate::backlight`].

use esp_hal::Async;
use esp_hal::gpio::Level;
use esp_hal::peripherals::GPIO21;
use esp_hal::rmt::{Channel, ConfigError, Error, PulseCode, Tx, TxChannelConfig, TxChannelCreator};

/// RMT clock this driver's timings assume (`Rmt::new(rmt, 80 MHz)`), in MHz.
pub const RMT_MHZ: u32 = 80;

// WS2812 bit timings in 12.5 ns ticks (datasheet: T0H 0.4 µs, T0L 0.85 µs,
// T1H 0.8 µs, T1L 0.45 µs; ±150 ns tolerance).
const T0H: u16 = 32;
const T0L: u16 = 68;
const T1H: u16 = 64;
const T1L: u16 = 36;

/// The RGB LED.
pub struct RgbLed {
    ch: Channel<'static, Async, Tx>,
}

impl RgbLed {
    /// Configures an RMT TX channel for the LED. The `Rmt` the channel
    /// comes from must run at [`RMT_MHZ`].
    pub fn new(
        channel: impl TxChannelCreator<'static, Async>,
        pin: GPIO21<'static>,
    ) -> Result<Self, ConfigError> {
        let config = TxChannelConfig::default()
            .with_clk_divider(1)
            .with_idle_output(true)
            .with_idle_output_level(Level::Low);
        Ok(Self {
            ch: channel.configure_tx(&config)?.with_pin(pin),
        })
    }

    /// Sets the colour (8-bit channels). The LED is very bright; values of
    /// 16–64 are plenty indoors.
    pub async fn set(&mut self, red: u8, green: u8, blue: u8) -> Result<(), Error> {
        let mut codes = [PulseCode::end_marker(); 25];
        // WS2812 expects GRB, most significant bit first.
        let grb = u32::from_be_bytes([0, green, red, blue]);
        for (i, code) in codes[..24].iter_mut().enumerate() {
            *code = if grb >> (23 - i) & 1 == 1 {
                PulseCode::new(Level::High, T1H, Level::Low, T1L)
            } else {
                PulseCode::new(Level::High, T0H, Level::Low, T0L)
            };
        }
        // The idle-low line after the last bit (>50 µs before the next
        // update) latches the colour.
        self.ch.transmit(&codes).await
    }

    /// Turns the LED off.
    pub async fn off(&mut self) -> Result<(), Error> {
        self.set(0, 0, 0).await
    }
}
