//! Wi-Fi scanner: lists nearby access points on screen and over defmt,
//! along with heap usage, every 10 seconds.
//!
//! Run with `cargo run --release --bin wifi_scan --features wifi`.
#![no_std]
#![no_main]

extern crate alloc;

use core::fmt::Write as _;

use defmt::{info, warn};
use embassy_executor::Spawner;
use embassy_time::{Duration, Instant, Timer};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text};
use esp_backtrace as _;
use esp_hal::time::Duration as HalDuration;
use esp_println as _;
use oxstack_cardputer::radio::esp_radio::wifi::WifiController;
use oxstack_cardputer::radio::esp_radio::wifi::scan::{ScanConfig, ScanTypeConfig};
use oxstack_cardputer::radio::heap_usage;
use oxstack_cardputer::{self as bsp, display};

esp_bootloader_esp_idf::esp_app_desc!();

/// One on-screen line.
struct Line {
    buf: [u8; 40],
    len: usize,
}

impl core::fmt::Write for Line {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for c in s.chars() {
            // The 6x10 font is ASCII-only; replace anything else.
            let b = if c.is_ascii() && !c.is_ascii_control() {
                c as u8
            } else {
                b'?'
            };
            if self.len < self.buf.len() {
                self.buf[self.len] = b;
                self.len += 1;
            }
        }
        Ok(())
    }
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let board = bsp::init(bsp::default_config());
    let _backlight = board.backlight.init();
    let mut lcd = board.display.init().await.unwrap();
    let fb = display::framebuffer();

    let (used, free) = heap_usage();
    info!("heap before Wi-Fi: {} used, {} free", used, free);
    let mut wifi = WifiController::new(board.radio.wifi, Default::default()).unwrap();
    let (used, free) = heap_usage();
    info!("heap after Wi-Fi init: {} used, {} free", used, free);

    // esp-radio's default dwell (10–20 ms per channel) often misses APs,
    // since beacons come every ~102 ms; use ESP-IDF's active-scan maximum.
    let scan = ScanConfig::default()
        .with_max(32)
        .with_scan_type(ScanTypeConfig::Active {
            min: HalDuration::from_millis(30),
            max: HalDuration::from_millis(120),
        });

    loop {
        let started = Instant::now();
        let mut aps = match wifi.scan_async(&scan).await {
            Ok(aps) => aps,
            Err(e) => {
                warn!("scan failed: {}", e);
                Timer::after(Duration::from_secs(5)).await;
                continue;
            }
        };
        aps.sort_unstable_by_key(|ap| core::cmp::Reverse(ap.signal_strength));
        let (used, free) = heap_usage();
        info!(
            "{} networks in {} ms (heap {} used, {} free)",
            aps.len(),
            started.elapsed().as_millis(),
            used,
            free
        );
        for ap in &aps {
            info!(
                "  {=i8} dBm  ch {=u8}  {}  {}",
                ap.signal_strength, ap.channel, ap.ssid, ap.auth_method
            );
        }

        fb.clear(Rgb565::BLACK).unwrap();
        let mut title = Line {
            buf: [0; 40],
            len: 0,
        };
        let _ = write!(title, "Wi-Fi: {} networks", aps.len());
        draw(fb, &title, 0, Rgb565::CSS_ORANGE);
        for (i, ap) in aps.iter().take(12).enumerate() {
            let mut line = Line {
                buf: [0; 40],
                len: 0,
            };
            let ssid = if ap.ssid.is_empty() {
                "<hidden>"
            } else {
                ap.ssid.as_str()
            };
            let _ = write!(line, "{:>4} {:>2} {}", ap.signal_strength, ap.channel, ssid);
            let color = if ap.signal_strength > -60 {
                Rgb565::GREEN
            } else {
                Rgb565::WHITE
            };
            draw(fb, &line, 12 + 10 * i as i32, color);
        }
        lcd.flush(fb).await.unwrap();
        drop(aps);

        Timer::after(Duration::from_secs(10)).await;
    }
}

fn draw(fb: &mut display::Frame, line: &Line, y: i32, color: Rgb565) {
    let text = core::str::from_utf8(&line.buf[..line.len]).unwrap_or("");
    Text::with_baseline(
        text,
        Point::new(2, y),
        MonoTextStyle::new(&FONT_6X10, color),
        Baseline::Top,
    )
    .draw(fb)
    .unwrap();
}
