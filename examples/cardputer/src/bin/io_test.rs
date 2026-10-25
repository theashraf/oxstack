//! RGB LED, G0 button and IR LED bring-up.
//!
//! - The RGB LED cycles red → green → blue → white → off, once a second.
//! - Pressing **G0** (the button on the side) counts presses on screen.
//! - Pressing **i** sends ten NEC frames over ~1 s on the IR LED: point a
//!   phone camera at the IR window to see it flicker purple. The code uses
//!   an extended address (0xBEEF) that no common device listens to.
#![no_std]
#![no_main]

use defmt::{info, warn};
use embassy_executor::Spawner;
use embassy_futures::select::{Either3, select3};
use embassy_time::{Duration, Ticker, Timer};
use embedded_graphics::mono_font::MonoTextStyleBuilder;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text};
use esp_backtrace as _;
use esp_hal::gpio::{Input, InputConfig, Pull};
use esp_println as _;
use oxstack_cardputer::gfx::TextBuf;
use oxstack_cardputer::input::Action;
use oxstack_cardputer::ir::protocols::nec;
use oxstack_cardputer::{self as bsp, display};

esp_bootloader_esp_idf::esp_app_desc!();

const COLOURS: [(&str, u8, u8, u8); 5] = [
    ("red", 32, 0, 0),
    ("green", 0, 32, 0),
    ("blue", 0, 0, 32),
    ("white", 24, 24, 24),
    ("off", 0, 0, 0),
];

fn row(fb: &mut display::Frame, y: i32, args: core::fmt::Arguments<'_>, color: Rgb565) {
    let mut l = TextBuf::<38>::format(args);
    l.pad(38); // always draw the full width so old text is overwritten
    let style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(color)
        .background_color(Rgb565::BLACK)
        .build();
    Text::with_baseline(l.as_str(), Point::new(2, y), style, Baseline::Top)
        .draw(fb)
        .unwrap();
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let mut board = bsp::init(bsp::default_config());
    // Full backlight also powers the RGB LED.
    let _backlight = board.backlight.init();
    let bus = board.sys_i2c.into_bus();
    let mut lcd = board.display.init().await.unwrap();
    let mut keyboard = board.keyboard.init(bus).await.unwrap();
    let fb = display::framebuffer();
    fb.clear(Rgb565::BLACK).unwrap();
    row(fb, 0, format_args!("oxstack io test"), Rgb565::CSS_ORANGE);
    row(
        fb,
        20,
        format_args!("LED cycles every second"),
        Rgb565::WHITE,
    );
    row(
        fb,
        30,
        format_args!("G0 (side button): press it"),
        Rgb565::WHITE,
    );
    row(
        fb,
        40,
        format_args!("i: IR burst (view via camera)"),
        Rgb565::WHITE,
    );

    // One RMT init gives both pulse-driven LEDs.
    let (mut led, mut ir) = board.rmt.init().unwrap();
    let mut button = Input::new(
        board.button.reborrow(),
        InputConfig::default().with_pull(Pull::Up),
    );

    let (mut colour, mut presses, mut bursts) = (0usize, 0u32, 0u32);
    let mut ticker = Ticker::every(Duration::from_secs(1));
    loop {
        let (name, ..) = COLOURS[colour];
        row(fb, 70, format_args!("LED: {:<8}", name), Rgb565::GREEN);
        row(
            fb,
            82,
            format_args!("G0 presses: {}", presses),
            Rgb565::GREEN,
        );
        row(
            fb,
            94,
            format_args!("IR bursts sent: {}", bursts),
            Rgb565::GREEN,
        );
        lcd.flush(fb).await.unwrap();

        match select3(
            ticker.next(),
            button.wait_for_falling_edge(),
            keyboard.next_event(),
        )
        .await
        {
            Either3::First(()) => {
                colour = (colour + 1) % COLOURS.len();
                let (name, r, g, b) = COLOURS[colour];
                if let Err(e) = led.set(r, g, b).await {
                    warn!("LED error: {}", e);
                }
                info!("LED {}", name);
            }
            Either3::Second(()) => {
                presses += 1;
                info!("G0 pressed ({})", presses);
                // Crude debounce: ignore bounces for 30 ms.
                Timer::after_millis(30).await;
            }
            Either3::Third(Ok(ev)) => {
                if ev.action() == Some(Action::Char('i')) {
                    for _ in 0..10 {
                        let frame = nec::encode_extended(0xBEEF, 0x01);
                        if let Err(e) = ir.send(&frame).await {
                            warn!("IR error: {}", e);
                        }
                        Timer::after_millis(40).await;
                    }
                    bursts += 1;
                    info!("IR burst {} sent", bursts);
                }
            }
            Either3::Third(Err(_)) => warn!("keyboard error"),
        }
    }
}
