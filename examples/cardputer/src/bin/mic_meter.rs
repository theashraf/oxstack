//! Microphone level meter. **Makes no sound**: only the codec's ADC path is
//! powered, and I²S TX is never started.
//!
//! Checks over defmt that audio actually flows (measured sample rate, live
//! and not clipped, left/right slot contents), then shows a live L/R level
//! meter on screen and logs levels four times a second.
#![no_std]
#![no_main]

use defmt::{info, warn};
use embassy_executor::Spawner;
use embassy_time::Instant;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::{Baseline, Text};
use esp_backtrace as _;
use esp_println as _;
use oxstack_cardputer::audio::{Error, MicGain, SampleRate};
use oxstack_cardputer::dsp::level::to_dbfs;
use oxstack_cardputer::{self as bsp, display};

esp_bootloader_esp_idf::esp_app_desc!();

const RATE: SampleRate = SampleRate::Hz48000;

/// Running statistics for one channel.
#[derive(Clone, Copy, Default)]
struct Stats {
    sum_sq: f64,
    sum: i64,
    peak: u16,
    min: i16,
    max: i16,
    n: u32,
}

impl Stats {
    fn add(&mut self, s: i16) {
        if self.n == 0 {
            (self.min, self.max) = (s, s);
        }
        self.sum_sq += f64::from(s) * f64::from(s);
        self.sum += i64::from(s);
        self.peak = self.peak.max(s.unsigned_abs());
        self.min = self.min.min(s);
        self.max = self.max.max(s);
        self.n += 1;
    }
    fn rms(&self) -> f32 {
        if self.n == 0 {
            0.0
        } else {
            (libm_sqrt(self.sum_sq / f64::from(self.n)) / 32768.0) as f32
        }
    }
    fn dc(&self) -> f32 {
        if self.n == 0 {
            0.0
        } else {
            self.sum as f32 / self.n as f32 / 32768.0
        }
    }
}

fn libm_sqrt(v: f64) -> f64 {
    // Newton's method from an f32 bit-hack seed; plenty for a meter.
    if v <= 0.0 {
        return 0.0;
    }
    let mut x = f64::from(f32::from_bits((((v as f32).to_bits()) >> 1) + 0x1FC0_0000));
    for _ in 0..4 {
        x = 0.5 * (x + v / x);
    }
    x
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let board = bsp::init(bsp::default_config());
    let _backlight = board.backlight.init();
    let bus = board.sys_i2c.into_bus();
    let mut lcd = board.display.init().await.unwrap();
    let fb = display::framebuffer();
    fb.clear(Rgb565::BLACK).unwrap();
    label(fb, "oxstack mic meter (silent)", 0, Rgb565::CSS_ORANGE);
    label(fb, "L", 40, Rgb565::WHITE);
    label(fb, "R", 70, Rgb565::WHITE);
    lcd.flush(fb).await.unwrap();

    let mut mic = board
        .audio
        .into_recorder(bus, RATE, MicGain::DB0)
        .await
        .unwrap();
    info!(
        "recorder started at {} Hz, PGA 0 dB (+24 dB digital scale)",
        RATE.hz()
    );

    // Let the ADC and DC-offset filter settle, then measure the true rate.
    let mut buf = [0i16; 1024];
    let settle = Instant::now();
    while settle.elapsed().as_millis() < 300 {
        let _ = mic.read(&mut buf).await;
    }

    let (mut left, mut right) = (Stats::default(), Stats::default());
    let (mut frames, mut overruns) = (0u64, 0u32);
    let start = Instant::now();
    while start.elapsed().as_millis() < 2000 {
        match mic.read(&mut buf).await {
            Ok(n) => {
                for f in buf[..n].chunks_exact(2) {
                    left.add(f[0]);
                    right.add(f[1]);
                }
                frames += (n / 2) as u64;
            }
            Err(Error::Overrun) => overruns += 1,
            Err(e) => defmt::panic!("audio error: {}", e),
        }
    }
    let rate = frames as f64 * 1e6 / start.elapsed().as_micros() as f64;
    info!(
        "measured rate = {} Hz over {} frames, {} overrun(s)",
        rate as u32, frames, overruns
    );
    for (name, s) in [("L", &left), ("R", &right)] {
        info!(
            "{}: rms {} dBFS, peak {} dBFS, dc {}, range [{}, {}]",
            name,
            to_dbfs(s.rms()),
            to_dbfs(f32::from(s.peak) / 32768.0),
            s.dc(),
            s.min,
            s.max
        );
    }

    let rate_ok = (rate - f64::from(RATE.hz())).abs() < f64::from(RATE.hz()) * 0.005;
    let live = left.max != left.min;
    let unclipped = left.peak < 32767;
    let checks = [
        (rate_ok, "sample rate within 0.5%"),
        (overruns == 0, "no overruns"),
        (live, "left slot carries a changing signal"),
        (unclipped, "left slot not clipped"),
    ];
    for (ok, what) in checks {
        if ok {
            info!("PASS  {}", what);
        } else {
            warn!("FAIL  {}", what);
        }
    }

    // Live meter.
    let mut window = (Stats::default(), Stats::default());
    let mut since = Instant::now();
    loop {
        match mic.read(&mut buf).await {
            Ok(n) => {
                for f in buf[..n].chunks_exact(2) {
                    window.0.add(f[0]);
                    window.1.add(f[1]);
                }
            }
            Err(Error::Overrun) => warn!("overrun"),
            Err(e) => defmt::panic!("audio error: {}", e),
        }
        if since.elapsed().as_millis() >= 250 {
            let (l, r) = (to_dbfs(window.0.rms()), to_dbfs(window.1.rms()));
            info!("L {} dBFS  R {} dBFS", l, r);
            bar(fb, 40, l);
            bar(fb, 70, r);
            lcd.flush(fb).await.unwrap();
            window = (Stats::default(), Stats::default());
            since = Instant::now();
        }
    }
}

fn label(fb: &mut display::Frame, text: &str, y: i32, color: Rgb565) {
    Text::with_baseline(
        text,
        Point::new(2, y),
        MonoTextStyle::new(&FONT_6X10, color),
        Baseline::Top,
    )
    .draw(fb)
    .unwrap();
}

/// A bar spanning -90..0 dBFS.
fn bar(fb: &mut display::Frame, y: i32, dbfs: f32) {
    let full = display::WIDTH as u32 - 20;
    let frac = ((dbfs + 90.0) / 90.0).clamp(0.0, 1.0);
    let len = (frac * full as f32) as u32;
    let color = if dbfs > -6.0 {
        Rgb565::RED
    } else {
        Rgb565::GREEN
    };
    Rectangle::new(Point::new(14, y), Size::new(full, 12))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::CSS_DARK_SLATE_GRAY))
        .draw(fb)
        .unwrap();
    Rectangle::new(Point::new(14, y), Size::new(len, 12))
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(fb)
        .unwrap();
}
