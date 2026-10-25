//! LCD backlight on GPIO38, dimmed with LEDC PWM.
//!
//! **GPIO38 also switches power to the RGB LED.** A WS2812 loses its state
//! whenever its supply drops, so any dimming below full brightness chops the
//! LED's power at the PWM rate. Keep the backlight at [`Backlight::MAX`]
//! while using the RGB LED. *(Inferred from the schematic and M5Stack's docs;
//! not yet measured on hardware.)*
//!
//! The driver uses LEDC low-speed timer 0 and channel 0. Dropping a
//! [`Backlight`] leaves the PWM running at its last level.

use embassy_sync::once_lock::OnceLock;
use esp_hal::gpio::DriveMode;
use esp_hal::ledc::channel::{self, Channel, ChannelHW, ChannelIFace};
use esp_hal::ledc::timer::{self, LSClockSource, Timer, TimerIFace};
use esp_hal::ledc::{LSGlobalClkSource, Ledc, LowSpeed};
use esp_hal::peripherals::{GPIO38, LEDC};
use esp_hal::time::Rate;

/// PWM frequency: well above visible flicker and within 8-bit LEDC range
/// from the 80 MHz APB clock.
pub const PWM_FREQUENCY: Rate = Rate::from_khz(2);

/// LEDC peripheral and the backlight pin.
#[allow(missing_docs)]
pub struct BacklightResources<'d> {
    pub ledc: LEDC<'d>,
    pub pin: GPIO38<'d>,
}
reborrow_group!(BacklightResources { ledc, pin });

/// The LEDC timer, configured once and shared by every [`Backlight`].
///
/// A `Channel` holds a reference to its timer for its whole life, so the
/// timer must outlive any `Backlight`: it lives in a static.
struct SharedTimer(Timer<'static, LowSpeed>);

// SAFETY: after `configure` (which runs once, before the value is
// published) the timer is only read through `&self`: its fields are plain
// values and the register block is accessed by volatile MMIO reads.
unsafe impl Sync for SharedTimer {}

fn timer() -> &'static Timer<'static, LowSpeed> {
    static TIMER: OnceLock<SharedTimer> = OnceLock::new();
    &TIMER
        .get_or_init(|| {
            // SAFETY: `BacklightResources` owns the real `LEDC` token and is
            // borrowed for the life of every `Backlight`; this alias only
            // configures timer 0, which the token's owner is told not to use.
            let mut ledc = Ledc::new(unsafe { LEDC::steal() });
            ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);
            let mut t = ledc.timer::<LowSpeed>(timer::Number::Timer0);
            t.configure(timer::config::Config {
                duty: timer::config::Duty::Duty8Bit,
                clock_source: LSClockSource::APBClk,
                frequency: PWM_FREQUENCY,
            })
            .expect("2 kHz at 8 bits is reachable from APB");
            SharedTimer(t)
        })
        .0
}

impl<'d> BacklightResources<'d> {
    /// Starts the backlight at full brightness.
    pub fn init(&mut self) -> Backlight<'_> {
        let mut ch = Channel::<LowSpeed>::new(channel::Number::Channel0, self.pin.reborrow());
        ch.configure(channel::config::Config {
            timer: timer(),
            duty_pct: 100,
            drive_mode: DriveMode::PushPull,
        })
        .expect("valid LEDC channel configuration");
        let mut backlight = Backlight {
            ch,
            level: Backlight::MAX,
        };
        backlight.set(Backlight::MAX);
        backlight
    }
}

/// Backlight brightness control.
pub struct Backlight<'d> {
    ch: Channel<'d, LowSpeed>,
    level: u8,
}

impl Backlight<'_> {
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
