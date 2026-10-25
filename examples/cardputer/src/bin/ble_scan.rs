//! Bluetooth LE scanner: lists advertising devices (name, signal, address)
//! on screen and over defmt, in 3-second scan rounds.
//!
//! Run with `cargo run --release --bin ble_scan --features ble`.
#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::Duration;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text};
use esp_backtrace as _;
use esp_println as _;
use oxstack_cardputer::ble::{self, trouble_host};
use oxstack_cardputer::gfx::text;
use oxstack_cardputer::radio::heap_usage;
use oxstack_cardputer::{self as bsp, display};
use trouble_host::Address;

esp_bootloader_esp_idf::esp_app_desc!();

const MAX_DEVICES: usize = 32;

/// One seen device. The table is fixed-size: a new address evicts the
/// weakest entry once it is full.
#[derive(Clone, Copy)]
struct Device {
    address: Address,
    rssi: i8,
    name: [u8; 20],
    name_len: u8,
}

impl Device {
    fn name(&self) -> &str {
        core::str::from_utf8(&self.name[..usize::from(self.name_len)]).unwrap_or("?")
    }
}

fn record(table: &mut [Option<Device>; MAX_DEVICES], adv: &ble::Advertisement<'_>) -> bool {
    let slot = table
        .iter()
        .position(|d| d.is_some_and(|d| d.address.addr == adv.address.addr));
    let (i, new) = match slot {
        Some(i) => (i, false),
        None => {
            let i = table.iter().position(Option::is_none).unwrap_or_else(|| {
                (0..MAX_DEVICES)
                    .min_by_key(|&i| table[i].map_or(i8::MAX, |d| d.rssi))
                    .unwrap()
            });
            table[i] = Some(Device {
                address: adv.address,
                rssi: adv.rssi,
                name: [0; 20],
                name_len: 0,
            });
            (i, true)
        }
    };
    let d = table[i].as_mut().unwrap();
    d.rssi = adv.rssi;
    // Names often arrive only in the scan response: keep the first seen.
    if d.name_len == 0
        && let Some(n) = adv.name()
    {
        let len = n.len().min(d.name.len());
        d.name[..len].copy_from_slice(&n.as_bytes()[..len]);
        d.name_len = len as u8;
    }
    new
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let mut board = bsp::init(bsp::default_config());
    let _backlight = board.backlight.init();
    let mut lcd = board.display.init().await.unwrap();
    let fb = display::framebuffer();

    // Controller and stack once per boot; scans as often as we like.
    let controller = ble::controller(board.radio.bt.reborrow()).unwrap();
    let mut resources = ble::ScanResources::new();
    let stack = ble::stack(controller, &mut resources);
    info!("our address: {}", ble::address());
    let (used, free) = heap_usage();
    info!("heap after BLE init: {} used, {} free", used, free);

    let mut table: [Option<Device>; MAX_DEVICES] = [None; MAX_DEVICES];
    loop {
        let mut new = 0;
        ble::scan(&stack, Duration::from_secs(3), |adv| {
            if record(&mut table, adv) {
                new += 1;
            }
        })
        .await
        .unwrap();

        let mut list = table;
        list.sort_unstable_by_key(|d| core::cmp::Reverse(d.map_or(i8::MIN, |d| d.rssi)));
        let total = list.iter().flatten().count();
        info!("{} devices ({} new)", total, new);
        for d in list.iter().flatten().take(8) {
            info!("  {=i8} dBm  {}  {}", d.rssi, d.address, d.name());
        }

        fb.clear(Rgb565::BLACK).unwrap();
        draw(
            fb,
            &text!(40, "BLE: {} devices", total),
            0,
            Rgb565::CSS_ORANGE,
        );
        for (i, d) in list.iter().flatten().take(12).enumerate() {
            let a = d.address.addr.raw();
            let name = if d.name_len == 0 { "-" } else { d.name() };
            let l = text!(40, "{:>4} {:02x}{:02x} {}", d.rssi, a[1], a[0], name);
            let color = if d.rssi > -60 {
                Rgb565::GREEN
            } else {
                Rgb565::WHITE
            };
            draw(fb, &l, 12 + 10 * i as i32, color);
        }
        lcd.flush(fb).await.unwrap();
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
