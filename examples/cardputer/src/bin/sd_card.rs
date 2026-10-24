//! microSD check. **Read-only**: nothing on the card is modified.
//!
//! Initialises the card, reports type and capacity, measures raw sequential
//! read speed, mounts the first FAT volume and lists the root directory, on
//! screen and over defmt.
#![no_std]
#![no_main]

use core::fmt::Write as _;
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
use oxstack_cardputer::sd::fs::{
    Block, BlockCount, BlockDevice, BlockIdx, VolumeIdx, VolumeManager,
};
use oxstack_cardputer::sd::{FixedTime, Sd};
use oxstack_cardputer::{self as bsp, display};

esp_bootloader_esp_idf::esp_app_desc!();

/// Fixed-capacity text line (ASCII only; the font has no other glyphs).
struct Line {
    buf: [u8; 40],
    len: usize,
}

impl Line {
    const fn new() -> Self {
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

/// Enforces read-only access: every write fails before reaching the card,
/// so neither this example nor the FAT library can modify it.
struct ReadOnly(Sd);

impl BlockDevice for ReadOnly {
    type Error = oxstack_cardputer::sd::Error;

    fn read(&self, blocks: &mut [Block], start: BlockIdx) -> Result<(), Self::Error> {
        self.0.read(blocks, start)
    }

    fn write(&self, _: &[Block], start: BlockIdx) -> Result<(), Self::Error> {
        warn!("blocked a write to block {} (read-only)", start.0);
        Err(oxstack_cardputer::sd::Error::WriteError)
    }

    fn num_blocks(&self) -> Result<BlockCount, Self::Error> {
        self.0.num_blocks()
    }
}

/// Reads `total` blocks sequentially, `N` at a time; returns KB/s.
fn read_speed<const N: usize>(sd: &Sd, total: u32) -> Option<u32> {
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
    let board = bsp::init(bsp::default_config());
    let _backlight = board.backlight.init();
    let mut lcd = board.display.init().await.unwrap();
    let fb = display::framebuffer();
    fb.clear(Rgb565::BLACK).unwrap();
    let mut screen = Screen { fb, row: 0 };
    screen.line("oxstack SD card (read-only)", Rgb565::CSS_ORANGE);

    let sd = match board.spi_bus.into_sd() {
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
    let mut l = Line::new();
    let _ = write!(l, "{:?}  {} MB", sd.card_type(), bytes / 1_000_000);
    info!("PASS  SD init: {}", l.as_str());
    screen.line(l.as_str(), Rgb565::GREEN);

    // 2 MB sequential read, 8 blocks (4 KB) per call.
    match read_speed::<8>(&sd, 4096) {
        Some(kbps) => {
            let mut l = Line::new();
            let _ = write!(l, "raw read: {} KB/s", kbps);
            info!("PASS  {}", l.as_str());
            screen.line(l.as_str(), Rgb565::GREEN);
        }
        None => warn!("FAIL  raw read"),
    }

    let fs = VolumeManager::<_, _, 4, 4, 1>::new(ReadOnly(sd), FixedTime);
    match fs.open_volume(VolumeIdx(0)) {
        Ok(volume) => {
            info!("PASS  FAT volume 0 mounted");
            let root = volume.open_root_dir().unwrap();
            let mut count = 0;
            let listed = root.iterate_dir(|entry| {
                if entry.attributes.is_volume() {
                    return ControlFlow::Continue(());
                }
                let mut l = Line::new();
                if entry.attributes.is_directory() {
                    let _ = write!(l, "  {}/", entry.name);
                } else {
                    let _ = write!(l, "  {} {}", entry.name, entry.size);
                }
                info!("{}", l.as_str());
                if screen.row < 13 {
                    screen.line(l.as_str(), Rgb565::WHITE);
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
