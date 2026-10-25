//! Wi-Fi scanner: lists nearby access points on screen and over defmt
//! every 10 seconds, switching the radio off in between.
//!
//! Run with `cargo run --release --bin wifi_scan --features wifi`.
#![no_std]
#![no_main]

use defmt::{info, warn};
use embassy_executor::Spawner;
use embassy_time::{Duration, Instant, Timer};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text};
use esp_backtrace as _;
use esp_println as _;
use oxstack_cardputer::gfx::text;
use oxstack_cardputer::radio::heap_usage;
use oxstack_cardputer::{self as bsp, display, net};

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let mut board = bsp::init(bsp::default_config());
    let _backlight = board.backlight.init();
    let mut lcd = board.display.init().await.unwrap();
    let fb = display::framebuffer();

    loop {
        let (used, free) = heap_usage();
        info!("heap with radio off: {} used, {} free", used, free);

        // The scanner borrows the radio and releases it when dropped, so
        // this loop body is exactly what a menu's "Wi-Fi" app would run.
        let started = Instant::now();
        let aps = match net::scanner(board.radio.wifi.reborrow()) {
            Ok(mut scanner) => scanner.scan().await,
            Err(e) => Err(e),
        };
        let aps = match aps {
            Ok(aps) => aps,
            Err(e) => {
                warn!("scan failed: {}", e);
                Timer::after(Duration::from_secs(5)).await;
                continue;
            }
        };
        info!(
            "{} networks in {} ms",
            aps.len(),
            started.elapsed().as_millis()
        );
        for ap in &aps {
            info!(
                "  {=i8} dBm  ch {=u8}  {}  {}",
                ap.signal_strength, ap.channel, ap.ssid, ap.auth_method
            );
        }

        fb.clear(Rgb565::BLACK).unwrap();
        draw(
            fb,
            &text!(40, "Wi-Fi: {} networks", aps.len()),
            0,
            Rgb565::CSS_ORANGE,
        );
        for (i, ap) in aps.iter().take(12).enumerate() {
            let ssid = if ap.ssid.is_empty() {
                "<hidden>"
            } else {
                ap.ssid.as_str()
            };
            let line = text!(40, "{:>4} {:>2} {}", ap.signal_strength, ap.channel, ssid);
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

fn draw(fb: &mut display::Frame, line: &str, y: i32, color: Rgb565) {
    Text::with_baseline(
        line,
        Point::new(2, y),
        MonoTextStyle::new(&FONT_6X10, color),
        Baseline::Top,
    )
    .draw(fb)
    .unwrap();
}
