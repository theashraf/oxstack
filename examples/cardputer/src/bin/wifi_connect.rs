//! Joins Wi-Fi, gets an address by DHCP, syncs the time over SNTP, and
//! shows a live clock with link details.
//!
//! Credentials come from the git-ignored `examples/cardputer/wifi.toml`
//! (copy `wifi.toml.example`). Without it the firmware still builds and
//! shows setup instructions on screen.
//!
//! Run with `cargo run --release --bin wifi_connect --features wifi`.
#![no_std]
#![no_main]

use core::fmt::Write as _;

use defmt::{info, warn};
use embassy_executor::Spawner;
use embassy_time::{Duration, Instant, Ticker, Timer, with_timeout};
use embedded_graphics::mono_font::ascii::{FONT_6X10, FONT_10X20};
use embedded_graphics::mono_font::{MonoFont, MonoTextStyleBuilder};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text};
use esp_backtrace as _;
use esp_println as _;
use oxstack_cardputer::net::{self, Credentials, WallClock, link};
use oxstack_cardputer::radio::heap_usage;
use oxstack_cardputer::{self as bsp, display};

esp_bootloader_esp_idf::esp_app_desc!();

const SSID: Option<&str> = option_env!("OXSTACK_WIFI_SSID");
const PASSWORD: &str = match option_env!("OXSTACK_WIFI_PASSWORD") {
    Some(p) => p,
    None => "",
};

/// Re-sync the clock this often (the crystal drifts a few seconds a day).
const RESYNC: Duration = Duration::from_secs(3600);

struct Line {
    buf: [u8; 40],
    len: usize,
}

impl Line {
    fn new() -> Self {
        Self {
            buf: [0; 40],
            len: 0,
        }
    }
    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

impl core::fmt::Write for Line {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for c in s.chars() {
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

fn text(fb: &mut display::Frame, s: &str, x: i32, y: i32, font: &MonoFont<'_>, color: Rgb565) {
    let style = MonoTextStyleBuilder::new()
        .font(font)
        .text_color(color)
        .background_color(Rgb565::BLACK)
        .build();
    Text::with_baseline(s, Point::new(x, y), style, Baseline::Top)
        .draw(fb)
        .unwrap();
}

fn utc_offset_minutes() -> i32 {
    option_env!("OXSTACK_UTC_OFFSET_MINUTES")
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let board = bsp::init(bsp::default_config());
    let _backlight = board.backlight.init();
    let mut lcd = board.display.init().await.unwrap();
    let fb = display::framebuffer();
    fb.clear(Rgb565::BLACK).unwrap();
    text(
        fb,
        "oxstack wifi_connect",
        2,
        0,
        &FONT_6X10,
        Rgb565::CSS_ORANGE,
    );

    let Some(ssid) = SSID else {
        warn!("no wifi.toml: copy examples/cardputer/wifi.toml.example to wifi.toml and rebuild");
        text(fb, "No Wi-Fi credentials.", 2, 20, &FONT_6X10, Rgb565::RED);
        text(
            fb,
            "Copy wifi.toml.example to",
            2,
            40,
            &FONT_6X10,
            Rgb565::WHITE,
        );
        text(
            fb,
            "examples/cardputer/wifi.toml",
            2,
            50,
            &FONT_6X10,
            Rgb565::WHITE,
        );
        text(fb, "and rebuild.", 2, 60, &FONT_6X10, Rgb565::WHITE);
        lcd.flush(fb).await.unwrap();
        loop {
            Timer::after_secs(60).await;
        }
    };

    let mut l = Line::new();
    let _ = write!(l, "Joining {}", ssid);
    text(fb, l.as_str(), 2, 20, &FONT_6X10, Rgb565::WHITE);
    lcd.flush(fb).await.unwrap();

    let stack = net::start_station(
        spawner,
        board.radio.wifi,
        Credentials {
            ssid,
            password: PASSWORD,
        },
    )
    .expect("valid credentials");
    let started = Instant::now();

    // Wait for association + DHCP, reporting progress.
    let mut last = link::State::Idle;
    while with_timeout(Duration::from_millis(250), stack.wait_config_up())
        .await
        .is_err()
    {
        let state = link::state();
        if state != last {
            info!("link: {}", state);
            last = state;
        }
    }
    let cfg = stack.config_v4().expect("config is up");
    info!(
        "connected in {} ms: ip {}, gateway {}, dns {}",
        started.elapsed().as_millis(),
        cfg.address,
        cfg.gateway,
        cfg.dns_servers.as_slice()
    );
    let (used, free) = heap_usage();
    info!("heap: {} used, {} free", used, free);

    let mut ip = Line::new();
    let _ = write!(ip, "IP {}", cfg.address.address());

    let mut clock: Option<WallClock> = None;
    let mut synced_at = Instant::now();
    let mut ticker = Ticker::every(Duration::from_secs(1));
    let offset = utc_offset_minutes();
    fb.clear(Rgb565::BLACK).unwrap();

    loop {
        if clock.is_none() || synced_at.elapsed() > RESYNC {
            match net::sync_clock(stack, net::DEFAULT_NTP_SERVER).await {
                Ok(c) => {
                    info!("SNTP: unix {}", c.unix_seconds());
                    clock = Some(c);
                    synced_at = Instant::now();
                }
                Err(e) => warn!("SNTP failed: {}", e),
            }
        }

        let mut title = Line::new();
        let _ = write!(title, "{}  {}", ssid, link::rssi().map_or(0, i32::from));
        let mut t = title;
        let _ = write!(t, " dBm   ");
        text(fb, t.as_str(), 2, 0, &FONT_6X10, Rgb565::CSS_ORANGE);
        text(fb, ip.as_str(), 2, 12, &FONT_6X10, Rgb565::WHITE);

        if let Some(c) = clock {
            let now = c.now(offset);
            let mut hms = Line::new();
            let _ = write!(hms, "{:02}:{:02}:{:02}", now.hour, now.minute, now.second);
            text(fb, hms.as_str(), 2, 50, &FONT_10X20, Rgb565::GREEN);
            let mut date = Line::new();
            let _ = write!(
                date,
                "{} {}-{:02}-{:02}  UTC{:+}h{:02}",
                now.weekday_name(),
                now.year,
                now.month,
                now.day,
                offset / 60,
                offset.unsigned_abs() % 60
            );
            text(fb, date.as_str(), 2, 80, &FONT_6X10, Rgb565::WHITE);
            if now.second == 0 {
                info!(
                    "{}-{:02}-{:02} {:02}:{:02}:{:02}  rssi {}",
                    now.year,
                    now.month,
                    now.day,
                    now.hour,
                    now.minute,
                    now.second,
                    link::rssi()
                );
            }
        } else {
            text(fb, "syncing time...", 2, 50, &FONT_6X10, Rgb565::WHITE);
        }
        lcd.flush(fb).await.unwrap();
        ticker.next().await;
    }
}
