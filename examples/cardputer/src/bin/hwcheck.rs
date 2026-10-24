//! Hardware self-test that needs no human input.
//!
//! Checks each on-board chip it can verify by itself, logs PASS/FAIL over
//! defmt, draws the results on screen, then streams live IMU and battery
//! readings once per second.
//!
//! - I²C: all three internal devices answer.
//! - TCA8418: keypad registers read back as configured.
//! - ES8311: chip-ID registers.
//! - BMI270: config upload accepted; at rest |accel| ≈ 1 g and gyro ≈ 0.
//! - Battery: voltage within the Li-ion range.
#![no_std]
#![no_main]

use core::fmt::Write as _;

use defmt::{info, warn};
use embassy_executor::Spawner;
use embassy_time::{Duration, Ticker, Timer};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::{Baseline, Text};
use embedded_hal_async::i2c::I2c as _;
use esp_backtrace as _;
use esp_println as _;
use oxstack_cardputer::pins::sys_i2c;
use oxstack_cardputer::{self as bsp, display, i2c, imu};

esp_bootloader_esp_idf::esp_app_desc!();

/// Fixed-capacity line buffer for on-screen text.
struct Line<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> Line<N> {
    const fn new() -> Self {
        Self {
            buf: [0; N],
            len: 0,
        }
    }
    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

impl<const N: usize> core::fmt::Write for Line<N> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let n = s.len().min(N - self.len);
        self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

struct Report<'a> {
    fb: &'a mut display::Frame,
    row: i32,
    failures: u32,
}

impl Report<'_> {
    fn check(&mut self, ok: bool, what: &str) {
        let (tag, color) = if ok {
            ("PASS", Rgb565::GREEN)
        } else {
            ("FAIL", Rgb565::RED)
        };
        if ok {
            info!("PASS  {}", what);
        } else {
            warn!("FAIL  {}", what);
            self.failures += 1;
        }
        let y = 12 + 10 * self.row;
        Text::with_baseline(
            tag,
            Point::new(2, y),
            MonoTextStyle::new(&FONT_6X10, color),
            Baseline::Top,
        )
        .draw(self.fb)
        .unwrap();
        Text::with_baseline(
            what,
            Point::new(32, y),
            MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE),
            Baseline::Top,
        )
        .draw(self.fb)
        .unwrap();
        self.row += 1;
    }
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let board = bsp::init(bsp::default_config());
    let _backlight = board.backlight.init();
    let bus = board.sys_i2c.into_bus();
    let mut lcd = board.display.init().await.unwrap();
    let fb = display::framebuffer();
    fb.clear(Rgb565::BLACK).unwrap();
    Text::with_baseline(
        "oxstack hwcheck",
        Point::new(2, 0),
        MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_ORANGE),
        Baseline::Top,
    )
    .draw(fb)
    .unwrap();

    let mut raw = i2c::device(bus);
    let mut r = Report {
        fb,
        row: 0,
        failures: 0,
    };

    let mut present = 0;
    for addr in [
        sys_i2c::ADDR_ES8311,
        sys_i2c::ADDR_TCA8418,
        sys_i2c::ADDR_BMI270,
    ] {
        if raw.write(addr, &[]).await.is_ok() {
            present += 1;
        }
    }
    r.check(present == 3, "I2C: 0x18 0x34 0x69 present");

    let kb = board.keyboard.init(bus).await;
    // The TCA8418 auto-increments only with CFG.AI set (we leave it off),
    // so each register is read separately.
    let mut regs = [0u8; 3];
    let mut kp_ok = true;
    for (i, reg) in [0x1D, 0x1E, 0x1F].into_iter().enumerate() {
        kp_ok &= raw
            .write_read(sys_i2c::ADDR_TCA8418, &[reg], &mut regs[i..i + 1])
            .await
            .is_ok();
    }
    info!("TCA8418 KP_GPIO1..3 = {=[u8]:#04x}", regs);
    r.check(
        kb.is_ok() && kp_ok && regs == [0x7F, 0xFF, 0x00],
        "TCA8418: 7x8 matrix configured",
    );

    // CHD1/CHD2/CHVER at 0xFD..0xFF, read one at a time (no burst reads).
    let mut id = [0u8; 3];
    let mut id_ok = true;
    for (i, reg) in [0xFD, 0xFE, 0xFF].into_iter().enumerate() {
        id_ok &= raw
            .write_read(sys_i2c::ADDR_ES8311, &[reg], &mut id[i..i + 1])
            .await
            .is_ok();
    }
    info!("ES8311 CHD1 CHD2 CHVER = {=[u8]:#04x}", id);
    r.check(id_ok && id[..2] == [0x83, 0x11], "ES8311: chip ID 83 11");

    let mut imu = match board.imu.init(bus, imu::Config::default()).await {
        Ok(imu) => {
            r.check(true, "BMI270: config upload accepted");
            Some(imu)
        }
        Err(e) => {
            warn!("BMI270 init error: {}", e);
            r.check(false, "BMI270: config upload accepted");
            None
        }
    };
    if let Some(imu) = imu.as_mut() {
        Timer::after_millis(50).await; // first samples after power-up
        let (mut amag, mut gmax, n) = (0.0f32, 0.0f32, 50);
        for _ in 0..n {
            let s = imu.read().await.unwrap();
            let [x, y, z] = s.accel;
            amag += libm_sqrt(x * x + y * y + z * z);
            for g in s.gyro {
                gmax = gmax.max(g.abs());
            }
            Timer::after_millis(10).await;
        }
        amag /= n as f32;
        info!("BMI270 mean |a| = {} g, peak |gyro| = {} dps", amag, gmax);
        r.check((0.9..1.1).contains(&amag), "BMI270: |accel| ~ 1 g at rest");
        r.check(gmax < 10.0, "BMI270: gyro ~ 0 at rest");
        let t = imu.temperature().await.ok().flatten();
        info!("BMI270 temperature = {} C", t);
        r.check(
            t.is_some_and(|t| (5.0..70.0).contains(&t)),
            "BMI270: temperature plausible",
        );
    }

    let mut battery = board.battery.init();
    let mv = battery.millivolts().await;
    info!("battery = {} mV", mv);
    r.check((3000..=4400).contains(&mv), "Battery: 3.0-4.4 V");

    let failures = r.failures;
    info!("hwcheck done: {} failure(s)", failures);
    lcd.flush(fb).await.unwrap();

    // Live readings on the bottom line.
    let mut ticker = Ticker::every(Duration::from_secs(1));
    loop {
        let mut line = Line::<40>::new();
        let mv = battery.millivolts().await;
        if let Some(imu) = imu.as_mut()
            && let Ok(s) = imu.read().await
        {
            let [x, y, z] = s.accel;
            info!(
                "accel g = [{}, {}, {}]  gyro dps = {}  battery = {} mV",
                x, y, z, s.gyro, mv
            );
            let _ = write!(line, "a {:+.2} {:+.2} {:+.2}  {}mV", x, y, z, mv);
        }
        let y = display::HEIGHT as i32 - 11;
        Rectangle::new(Point::new(0, y), Size::new(display::WIDTH as u32, 11))
            .into_styled(PrimitiveStyle::with_fill(Rgb565::CSS_DARK_SLATE_GRAY))
            .draw(fb)
            .unwrap();
        Text::with_baseline(
            line.as_str(),
            Point::new(2, y + 1),
            MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE),
            Baseline::Top,
        )
        .draw(fb)
        .unwrap();
        lcd.flush(fb).await.unwrap();
        ticker.next().await;
    }
}

/// Square root without pulling in `libm`: Newton's method from a bit-hack
/// seed, accurate to f32 precision after three steps for our 0–16 g range.
fn libm_sqrt(v: f32) -> f32 {
    if v <= 0.0 {
        return 0.0;
    }
    let mut x = f32::from_bits((v.to_bits() >> 1) + 0x1FC0_0000);
    for _ in 0..3 {
        x = 0.5 * (x + v / x);
    }
    x
}
