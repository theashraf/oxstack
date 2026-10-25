//! A minimal typewriter: keyboard → screen, with a modifier status bar.
//!
//! Exercises the whole input stack: TCA8418 interrupt → FIFO drain → matrix
//! map → modifiers/layers → `Action`. Fn+` (Esc) clears the page. Every
//! key event is also logged over defmt.
#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::mono_font::{MonoTextStyle, MonoTextStyleBuilder};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::{Baseline, Text};
use esp_backtrace as _;
use esp_println as _;
use oxstack_cardputer::input::{Action, Modifiers};
use oxstack_cardputer::{self as bsp, display};

esp_bootloader_esp_idf::esp_app_desc!();

const COLS: usize = display::WIDTH / 6;
const ROWS: usize = (display::HEIGHT - 12) / 10;

struct Page {
    text: [[u8; COLS]; ROWS],
    row: usize,
    col: usize,
}

impl Page {
    const fn new() -> Self {
        Self {
            text: [[b' '; COLS]; ROWS],
            row: 0,
            col: 0,
        }
    }

    /// Applies an action; returns the rows that need redrawing.
    fn apply(&mut self, action: Action) -> core::ops::Range<usize> {
        let before = self.row;
        match action {
            Action::Char(c) if c.is_ascii() => {
                self.text[self.row][self.col] = c as u8;
                self.advance();
            }
            Action::Tab => {
                for _ in 0..4 {
                    self.text[self.row][self.col] = b' ';
                    self.advance();
                }
            }
            Action::Enter => self.newline(),
            Action::Backspace if self.col > 0 => {
                self.col -= 1;
                self.text[self.row][self.col] = b' ';
            }
            Action::Escape => {
                *self = Self::new();
                return 0..ROWS;
            }
            _ => {}
        }
        if self.row == before {
            before..before + 1
        } else {
            0..ROWS
        }
    }

    fn advance(&mut self) {
        self.col += 1;
        if self.col == COLS {
            self.newline();
        }
    }

    fn newline(&mut self) {
        self.col = 0;
        self.row += 1;
        if self.row == ROWS {
            self.text.copy_within(1.., 0);
            self.text[ROWS - 1] = [b' '; COLS];
            self.row = ROWS - 1;
        }
    }
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let mut board = bsp::init(bsp::default_config());
    let _backlight = board.backlight.init();
    let i2c = board.sys_i2c.into_bus();
    let mut lcd = board.display.init().await.unwrap();
    let mut keyboard = board.keyboard.init(i2c).await.unwrap();
    let fb = display::framebuffer();

    let mut page = Page::new();
    fb.clear(Rgb565::BLACK).unwrap();
    draw_rows(fb, &page, 0..ROWS);
    draw_status(fb, Modifiers::NONE);
    lcd.flush(fb).await.unwrap();
    info!("typewriter ready");

    loop {
        let ev = keyboard.next_event().await.unwrap();
        info!(
            "{} {} mods={}",
            ev.key,
            if ev.pressed { "down" } else { "up" },
            ev.mods
        );
        if let Some(action) = ev.action() {
            let dirty = page.apply(action);
            draw_rows(fb, &page, dirty);
        }
        draw_status(fb, ev.mods);
        lcd.flush(fb).await.unwrap();
    }
}

fn draw_rows(fb: &mut display::Frame, page: &Page, rows: core::ops::Range<usize>) {
    let style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(Rgb565::WHITE)
        .background_color(Rgb565::BLACK)
        .build();
    for r in rows {
        let line = core::str::from_utf8(&page.text[r]).unwrap_or("");
        Text::with_baseline(line, Point::new(0, 10 * r as i32), style, Baseline::Top)
            .draw(fb)
            .unwrap();
    }
    // Cursor underline.
    let at = Point::new(6 * page.col as i32, 10 * page.row as i32 + 9);
    Rectangle::new(at, Size::new(6, 1))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::CSS_ORANGE))
        .draw(fb)
        .unwrap();
}

fn draw_status(fb: &mut display::Frame, mods: Modifiers) {
    let y = display::HEIGHT as i32 - 11;
    Rectangle::new(Point::new(0, y), Size::new(display::WIDTH as u32, 11))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::CSS_DARK_SLATE_GRAY))
        .draw(fb)
        .unwrap();
    let mut x = 2;
    for (m, label) in [
        (Modifiers::SHIFT, "SHIFT"),
        (Modifiers::CTRL, "CTRL"),
        (Modifiers::ALT, "ALT"),
        (Modifiers::OPT, "OPT"),
        (Modifiers::FN, "FN"),
    ] {
        let color = if mods.contains(m) {
            Rgb565::CSS_ORANGE
        } else {
            Rgb565::CSS_GRAY
        };
        Text::with_baseline(
            label,
            Point::new(x, y + 1),
            MonoTextStyle::new(&FONT_6X10, color),
            Baseline::Top,
        )
        .draw(fb)
        .unwrap();
        x += 6 * label.len() as i32 + 8;
    }
}
