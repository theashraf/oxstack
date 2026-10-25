//! microSD check. **Read-only**: nothing on the card is modified. That is
//! the board crate's default, so the example needs no guard of its own.
//!
//! Initialises the card, reports type and capacity, measures raw sequential
//! read speed, mounts the first FAT volume and lists the root directory, on
//! screen and over defmt.
#![no_std]
#![no_main]

use core::ops::ControlFlow;

use defmt::{info, warn};
use embassy_executor::Spawner;
use embassy_time::{Instant, Timer};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text};
use esp_backtrace as _;
use esp_println as _;
use oxstack_cardputer::gfx::text;
use oxstack_cardputer::sd::fs::{Block, BlockDevice, BlockIdx, VolumeIdx, VolumeManager};
use oxstack_cardputer::sd::{FixedTime, Sd};
use oxstack_cardputer::{self as bsp, display};

esp_bootloader_esp_idf::esp_app_desc!();

struct Screen<'a> {
    fb: &'a mut display::Frame,
    row: i32,
}

impl Screen<'_> {
    fn line(&mut self, text: &str, color: Rgb565) {
        Text::with_baseline(
            text,
            Point::new(2, 10 * self.row),
            MonoTextStyle::new(&FONT_6X10, color),
            Baseline::Top,
        )
        .draw(self.fb)
        .unwrap();
        self.row += 1;
    }
}

/// Reads `total` blocks sequentially, `N` at a time; returns KB/s.
fn read_speed<const N: usize>(sd: &Sd<'_>, total: u32) -> Option<u32> {
    let mut blocks: [Block; N] = core::array::from_fn(|_| Block::new());
    let start = Instant::now();
    let mut idx = 0;
    while idx < total {
        sd.read(&mut blocks, BlockIdx(idx)).ok()?;
        idx += N as u32;
    }
    let us = start.elapsed().as_micros().max(1);
    Some((u64::from(total) * 512 * 1_000_000 / 1024 / us) as u32)
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let mut board = bsp::init(bsp::default_config());
    let _backlight = board.backlight.init();
    let mut lcd = board.display.init().await.unwrap();
    let fb = display::framebuffer();
    fb.clear(Rgb565::BLACK).unwrap();
    let mut screen = Screen { fb, row: 0 };
    screen.line("oxstack SD card (read-only)", Rgb565::CSS_ORANGE);

    let sd = match board.spi_bus.sd() {
        Ok(sd) => sd,
        Err(e) => {
            warn!("FAIL  SD init: {}", e);
            screen.line("FAIL: no card / no response", Rgb565::RED);
            lcd.flush(screen.fb).await.unwrap();
            loop {
                Timer::after_secs(60).await;
            }
        }
    };

    let bytes = sd.size_bytes().unwrap_or(0);
    let l = text!(40, "{:?}  {} MB", sd.card_type(), bytes / 1_000_000);
    info!("PASS  SD init: {}", l.as_str());
    screen.line(&l, Rgb565::GREEN);

    // 2 MB sequential read, 8 blocks (4 KB) per call.
    match read_speed::<8>(&sd, 4096) {
        Some(kbps) => {
            let l = text!(40, "raw read: {} KB/s", kbps);
            info!("PASS  {}", l.as_str());
            screen.line(&l, Rgb565::GREEN);
        }
        None => warn!("FAIL  raw read"),
    }

    // `sd` is read-only by default: the FAT library's bookkeeping write
    // (FSInfo) is refused with a warning and listing still works.
    let fs = VolumeManager::<_, _, 4, 4, 1>::new(sd, FixedTime);
    match fs.open_volume(VolumeIdx(0)) {
        Ok(volume) => {
            info!("PASS  FAT volume 0 mounted");
            let root = volume.open_root_dir().unwrap();
            let mut count = 0;
            let listed = root.iterate_dir(|entry| {
                if entry.attributes.is_volume() {
                    return ControlFlow::Continue(());
                }
                let l = if entry.attributes.is_directory() {
                    text!(40, "  {}/", entry.name)
                } else {
                    text!(40, "  {} {}", entry.name, entry.size)
                };
                info!("{}", l.as_str());
                if screen.row < 13 {
                    screen.line(&l, Rgb565::WHITE);
                }
                count += 1;
                ControlFlow::Continue(())
            });
            match listed {
                Ok(()) => info!("PASS  root directory: {} entries", count),
                Err(e) => warn!("FAIL  listing: {}", defmt::Debug2Format(&e)),
            }
        }
        Err(e) => {
            warn!("FAIL  mount: {}", defmt::Debug2Format(&e));
            screen.line("FAIL: no FAT volume", Rgb565::RED);
        }
    }
    lcd.flush(screen.fb).await.unwrap();

    loop {
        Timer::after_secs(60).await;
    }
}
