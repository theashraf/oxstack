//! LCD backlight on GPIO38, dimmed with LEDC PWM.
//!
//! **GPIO38 also switches power to the RGB LED.** A WS2812 loses its state
//! whenever its supply drops, so any dimming below full brightness chops the
//! LED's power at the PWM rate. Keep the backlight at [`Backlight::MAX`]
//! while using the RGB LED. *(Inferred from the schematic and M5Stack's docs;
//! not yet measured on hardware.)*

use esp_hal::gpio::DriveMode;
use esp_hal::ledc::channel::{self, Channel, ChannelHW, ChannelIFace};
use esp_hal::ledc::timer::{self, LSClockSource, Timer, TimerIFace};
use esp_hal::ledc::{LSGlobalClkSource, Ledc, LowSpeed};
use esp_hal::peripherals::{GPIO38, LEDC};
use esp_hal::time::Rate;
use static_cell::StaticCell;

/// PWM frequency: well above visible flicker and within 8-bit LEDC range
/// from the 80 MHz APB clock.
pub const PWM_FREQUENCY: Rate = Rate::from_khz(2);

/// LEDC peripheral and the backlight pin.
#[allow(missing_docs)]
pub struct BacklightResources {
    pub ledc: LEDC<'static>,
    pub pin: GPIO38<'static>,
}

impl BacklightResources {
    /// Starts the backlight at full brightness.
    ///
    /// Uses LEDC low-speed timer 0 and channel 0, and consumes the LEDC
    /// peripheral, so its other channels are not available.
    pub fn init(self) -> Backlight {
        static TIMER: StaticCell<Timer<'static, LowSpeed>> = StaticCell::new();

        let mut ledc = Ledc::new(self.ledc);
        ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);
        let mut t = ledc.timer::<LowSpeed>(timer::Number::Timer0);
        t.configure(timer::config::Config {
            duty: timer::config::Duty::Duty8Bit,
            clock_source: LSClockSource::APBClk,
            frequency: PWM_FREQUENCY,
        })
        .expect("2 kHz at 8 bits is reachable from APB");
        let timer = TIMER.init(t);

        let mut ch = ledc.channel(channel::Number::Channel0, self.pin);
        ch.configure(channel::config::Config {
            timer,
            duty_pct: 100,
            drive_mode: DriveMode::PushPull,
        })
        .expect("valid LEDC channel configuration");
        Backlight {
            ch,
            level: Backlight::MAX,
        }
    }
}

/// Backlight brightness control.
pub struct Backlight {
    ch: Channel<'static, LowSpeed>,
    level: u8,
}

impl Backlight {
    /// Full brightness: the pin is held steadily high (no PWM).
    pub const MAX: u8 = u8::MAX;

    /// Sets brightness, `0` (off) to [`Self::MAX`] (steady on).
    pub fn set(&mut self, level: u8) {
        // An 8-bit LEDC duty of 256 means "always high", so map 255 to 256
        // to keep full brightness free of PWM edges.
        let duty = if level == Self::MAX {
            1 << 8
        } else {
            u32::from(level)
        };
        self.ch.set_duty_hw(duty);
        self.level = level;
    }

    /// Current brightness.
    pub fn level(&self) -> u8 {
        self.level
    }
}
