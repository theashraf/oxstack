//! {{project-name}}: a Cardputer ADV firmware on oxstack.
//!
//! Typed characters appear on screen; Esc clears it.
#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embedded_graphics::mono_font::MonoTextStyleBuilder;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text};
use esp_backtrace as _;
use esp_println as _;
use oxstack_cardputer::gfx::text;
use oxstack_cardputer::input::Action;
use oxstack_cardputer::{self as bsp, display};

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    // `board` must stay alive and mutable: every driver borrows from it.
    let mut board = bsp::init(bsp::default_config());
    let _backlight = board.backlight.init();
    let i2c = board.sys_i2c.into_bus();
    let mut lcd = board.display.init().await.unwrap();
    let mut keyboard = board.keyboard.init(i2c).await.unwrap();
    let fb = display::framebuffer();

    let style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(Rgb565::WHITE)
        .background_color(Rgb565::BLACK)
        .build();
    let mut line = text!(40, "");
    fb.clear(Rgb565::BLACK).unwrap();
    Text::with_baseline("{{project-name}}", Point::new(2, 0), style, Baseline::Top)
        .draw(fb)
        .unwrap();
    lcd.flush(fb).await.unwrap();
    info!("ready");

    loop {
        let ev = keyboard.next_event().await.unwrap();
        match ev.action() {
            Some(Action::Char(c)) => {
                let _ = core::fmt::Write::write_char(&mut line, c);
            }
            Some(Action::Escape) => line.clear(),
            _ => {}
        }
        info!("{} {}", ev.key, if ev.pressed { "down" } else { "up" });
        Text::with_baseline(line.pad(40), Point::new(2, 20), style, Baseline::Top)
            .draw(fb)
            .unwrap();
        lcd.flush(fb).await.unwrap(); // sends only the rows that changed
    }
}
