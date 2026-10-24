//! Bluetooth LE scanner: lists advertising devices (name, signal, address)
//! on screen and over defmt.
//!
//! Run with `cargo run --release --bin ble_scan --features ble`.
#![no_std]
#![no_main]

use core::cell::RefCell;
use core::fmt::Write as _;

use defmt::info;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_time::{Duration, Instant, Timer};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text};
use esp_backtrace as _;
use esp_println as _;
use oxstack_cardputer::ble::{self, trouble_host};
use oxstack_cardputer::radio::heap_usage;
use oxstack_cardputer::{self as bsp, display};
use trouble_host::prelude::*;

esp_bootloader_esp_idf::esp_app_desc!();

const MAX_DEVICES: usize = 32;

#[derive(Clone, Copy)]
struct Device {
    addr: [u8; 6],
    rssi: i8,
    name: [u8; 20],
    name_len: u8,
    last_seen: Instant,
}

impl Device {
    fn name(&self) -> &str {
        core::str::from_utf8(&self.name[..usize::from(self.name_len)]).unwrap_or("?")
    }
}

/// Collects advertisement reports into a fixed table (no allocation).
struct Collector {
    devices: RefCell<heapless_table::Table>,
}

/// A tiny fixed-capacity table, so the example needs no extra crates.
mod heapless_table {
    use super::{Device, MAX_DEVICES};
    pub struct Table {
        pub items: [Option<Device>; MAX_DEVICES],
        pub new_since_print: u32,
    }
    impl Table {
        pub const fn new() -> Self {
            Self {
                items: [None; MAX_DEVICES],
                new_since_print: 0,
            }
        }
    }
}

impl EventHandler for Collector {
    fn on_adv_reports(&self, mut reports: LeAdvReportsIter<'_>) {
        let mut table = self.devices.borrow_mut();
        while let Some(Ok(report)) = reports.next() {
            let addr: [u8; 6] = report.addr.raw().try_into().unwrap_or([0; 6]);
            let mut name = None;
            for ad in AdStructure::decode(report.data).flatten() {
                if let AdStructure::CompleteLocalName(n) | AdStructure::ShortenedLocalName(n) = ad {
                    name = Some(n);
                }
            }
            let now = Instant::now();
            let slot = table
                .items
                .iter()
                .position(|d| d.is_some_and(|d| d.addr == addr));
            let slot = match slot {
                Some(i) => i,
                None => {
                    // New device: take a free slot, or evict the stalest.
                    let i = table
                        .items
                        .iter()
                        .position(Option::is_none)
                        .unwrap_or_else(|| {
                            (0..MAX_DEVICES)
                                .min_by_key(|&i| table.items[i].map(|d| d.last_seen))
                                .unwrap()
                        });
                    table.items[i] = Some(Device {
                        addr,
                        rssi: report.rssi,
                        name: [0; 20],
                        name_len: 0,
                        last_seen: now,
                    });
                    table.new_since_print += 1;
                    i
                }
            };
            let d = table.items[slot].as_mut().unwrap();
            d.rssi = report.rssi;
            d.last_seen = now;
            // Names often arrive only in the scan response, so keep the first one seen.
            if let Some(n) = name.filter(|_| d.name_len == 0) {
                let len = n.len().min(d.name.len());
                d.name[..len].copy_from_slice(&n[..len]);
                d.name_len = len as u8;
            }
        }
    }
}

struct Line {
    buf: [u8; 40],
    len: usize,
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

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let board = bsp::init(bsp::default_config());
    let _backlight = board.backlight.init();
    let mut lcd = board.display.init().await.unwrap();
    let fb = display::framebuffer();

    let controller = ble::controller(board.radio.bt).unwrap();
    let address = ble::address();
    info!("our address: {}", address);
    let mut resources: HostResources<_, DefaultPacketPool, 1, 1> = HostResources::new();
    let stack = trouble_host::new(controller, &mut resources)
        .set_random_address(address)
        .build();
    let mut runner = stack.runner();
    let mut scanner = Scanner::new(stack.central());
    let collector = Collector {
        devices: RefCell::new(heapless_table::Table::new()),
    };
    let (used, free) = heap_usage();
    info!("heap after BLE init: {} used, {} free", used, free);

    let ui = async {
        let config = ScanConfig {
            active: true, // ask for scan responses, which usually carry names
            phys: PhySet::M1,
            interval: Duration::from_millis(100),
            window: Duration::from_millis(100),
            ..Default::default()
        };
        let _session = scanner.scan(&config).await.unwrap();
        loop {
            Timer::after(Duration::from_secs(3)).await;
            let mut list: [Option<Device>; MAX_DEVICES] = collector.devices.borrow().items;
            list.sort_unstable_by_key(|d| core::cmp::Reverse(d.map_or(i8::MIN, |d| d.rssi)));
            let total = list.iter().flatten().count();
            let new = core::mem::take(&mut collector.devices.borrow_mut().new_since_print);
            let (used, _) = heap_usage();
            info!("{} devices ({} new), heap {} used", total, new, used);
            for d in list.iter().flatten().take(8) {
                let a = d.addr;
                info!(
                    "  {=i8} dBm  {=u8:02x}:{=u8:02x}:{=u8:02x}:{=u8:02x}:{=u8:02x}:{=u8:02x}  {}",
                    d.rssi,
                    a[5],
                    a[4],
                    a[3],
                    a[2],
                    a[1],
                    a[0],
                    d.name()
                );
            }

            fb.clear(Rgb565::BLACK).unwrap();
            let mut title = Line {
                buf: [0; 40],
                len: 0,
            };
            let _ = write!(title, "BLE: {} devices", total);
            draw(fb, &title, 0, Rgb565::CSS_ORANGE);
            for (i, d) in list.iter().flatten().take(12).enumerate() {
                let mut l = Line {
                    buf: [0; 40],
                    len: 0,
                };
                let name = if d.name_len == 0 { "-" } else { d.name() };
                let _ = write!(
                    l,
                    "{:>4} {:02x}{:02x} {}",
                    d.rssi, d.addr[1], d.addr[0], name
                );
                let color = if d.rssi > -60 {
                    Rgb565::GREEN
                } else {
                    Rgb565::WHITE
                };
                draw(fb, &l, 12 + 10 * i as i32, color);
            }
            lcd.flush(fb).await.unwrap();
        }
    };
    join(runner.run_with_handler(&collector), ui).await;
    unreachable!()
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
