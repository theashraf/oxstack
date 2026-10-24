//! Speaker bring-up, fully under keyboard control. **Silent until you press
//! a key.**
//!
//! - `Space`: play / stop a 440 Hz tone
//! - `1`–`5`: volume steps (-40, -30, -20, -12, -6 dB), starting at -40 dB
//! - `t`: one-second sweep 200 Hz → 2 kHz
//!
//! Plugging in headphones mutes the speaker amplifier in hardware.
#![no_std]
#![no_main]

use defmt::{info, warn};
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::mono_font::{MonoTextStyle, MonoTextStyleBuilder};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text};
use esp_backtrace as _;
use esp_println as _;
use oxstack_cardputer::audio::{Error, Player, SampleRate, Volume};
use oxstack_cardputer::dsp::tone::Sine;
use oxstack_cardputer::input::Action;
use oxstack_cardputer::{self as bsp, display};

esp_bootloader_esp_idf::esp_app_desc!();

const RATE: SampleRate = SampleRate::Hz48000;
/// Tone amplitude before the codec's digital volume: -6 dBFS.
const AMPLITUDE: f32 = 0.5;
/// Volume steps in dB relative to unity.
const STEPS_DB: [i16; 5] = [-40, -30, -20, -12, -6];
/// Samples per write: 5.3 ms of stereo audio at 48 kHz.
const BLOCK: usize = 512;

fn label(fb: &mut display::Frame, y: i32, s: &str, color: Rgb565) {
    let style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(color)
        .background_color(Rgb565::BLACK)
        .build();
    Text::with_baseline(s, Point::new(2, y), style, Baseline::Top)
        .draw(fb)
        .unwrap();
}

/// Fixed-capacity text line.
struct Line {
    buf: [u8; 38],
    len: usize,
}

impl core::fmt::Write for Line {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let n = s.len().min(self.buf.len() - self.len);
        self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

fn status(fb: &mut display::Frame, playing: bool, step: usize) {
    use core::fmt::Write as _;
    let mut l = Line {
        buf: [b' '; 38],
        len: 0,
    };
    let state = if playing { "PLAYING 440 Hz" } else { "stopped" };
    let _ = write!(l, "{:<16}vol {:>3} dB", state, STEPS_DB[step]);
    let color = if playing {
        Rgb565::GREEN
    } else {
        Rgb565::WHITE
    };
    // Pad to the full width so a shorter line overwrites a longer one.
    label(fb, 70, core::str::from_utf8(&l.buf).unwrap_or(""), color);
}

async fn play_block(player: &mut Player, osc: &mut Sine) {
    let mut block = [0i16; BLOCK];
    osc.fill(&mut block, 2);
    match player.write(&block).await {
        Ok(()) => {}
        Err(Error::Underrun) => warn!("underrun"),
        Err(e) => defmt::panic!("audio error: {}", e),
    }
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let board = bsp::init(bsp::default_config());
    let _backlight = board.backlight.init();
    let bus = board.sys_i2c.into_bus();
    let mut lcd = board.display.init().await.unwrap();
    let mut keyboard = board.keyboard.init(bus).await.unwrap();
    let fb = display::framebuffer();
    fb.clear(Rgb565::BLACK).unwrap();
    Text::with_baseline(
        "oxstack speaker test",
        Point::new(2, 0),
        MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_ORANGE),
        Baseline::Top,
    )
    .draw(fb)
    .unwrap();
    label(fb, 20, "Space  play/stop 440 Hz", Rgb565::WHITE);
    label(fb, 30, "1..5   volume -40..-6 dB", Rgb565::WHITE);
    label(fb, 40, "t      sweep 200 Hz-2 kHz", Rgb565::WHITE);

    let mut step = 0;
    let mut player = board
        .audio
        .into_player(bus, RATE, Volume::from_half_db(STEPS_DB[step] * 2))
        .await
        .unwrap();
    info!(
        "player ready, muted until first sound; volume {} dB",
        STEPS_DB[step]
    );
    status(fb, false, step);
    lcd.flush(fb).await.unwrap();

    let mut playing = false;
    let mut osc = Sine::new(440.0, RATE.hz(), AMPLITUDE);
    let silence = [0i16; BLOCK];

    loop {
        // Keep the stream fed (tone or silence) so the codec's BCLK-derived
        // clock never stops, and check the keyboard between blocks.
        let key = if playing {
            match select(keyboard.next_event(), play_block(&mut player, &mut osc)).await {
                Either::First(ev) => Some(ev),
                Either::Second(()) => None,
            }
        } else {
            match select(keyboard.next_event(), player.write(&silence)).await {
                Either::First(ev) => Some(ev),
                Either::Second(_) => None,
            }
        };
        let Some(Ok(ev)) = key else { continue };
        let Some(action) = ev.action() else { continue };
        match action {
            Action::Char(' ') => {
                playing = !playing;
                info!("tone {}", if playing { "on" } else { "off" });
            }
            Action::Char(c @ '1'..='5') => {
                step = (c as u8 - b'1') as usize;
                player
                    .set_volume(Volume::from_half_db(STEPS_DB[step] * 2))
                    .await
                    .unwrap();
                info!("volume {} dB", STEPS_DB[step]);
            }
            Action::Char('t') => {
                info!("sweep");
                // One second, retuned every block with continuous phase.
                let blocks = RATE.hz() as usize / (BLOCK / 2);
                let mut sweep = Sine::new(200.0, RATE.hz(), AMPLITUDE);
                let mut block = [0i16; BLOCK];
                for i in 0..blocks {
                    sweep.set_frequency(200.0 + 1800.0 * i as f32 / blocks as f32, RATE.hz());
                    sweep.fill(&mut block, 2);
                    let _ = player.write(&block).await;
                }
            }
            _ => {}
        }
        status(fb, playing, step);
        lcd.flush(fb).await.unwrap();
    }
}
