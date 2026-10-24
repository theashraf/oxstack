//! Display bring-up: alignment test, then a dirty-row animation benchmark.
//!
//! **Alignment:** the screen shows a 1-pixel white border and a coloured
//! square in each corner (red top-left, green top-right, blue bottom-left,
//! yellow bottom-right). All four edges and squares must be fully visible.
//! A missing or doubled edge means the panel offsets in
//! `oxstack_cardputer::display::PANEL` are wrong.
//!
//! **Benchmark:** logs the time of a full-frame flush, then animates a
//! square. Only the rows it touches are re-sent each frame.
#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::{Duration, Instant, Ticker};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use esp_backtrace as _;
use esp_println as _;
use oxstack_cardputer::{self as bsp, display};

esp_bootloader_esp_idf::esp_app_desc!();

const BOX: u32 = 16;

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let board = bsp::init(bsp::default_config());
    let _backlight = board.backlight.init();
    let mut lcd = board.display.init().await.unwrap();
    let fb = display::framebuffer();

    draw_alignment_test(fb);
    let start = Instant::now();
    lcd.flush(fb).await.unwrap();
    info!("full-frame flush: {} us", start.elapsed().as_micros());

    let arena = Rectangle::new(Point::new(8, 76), Size::new(display::WIDTH as u32 - 16, 48));
    let mut pos = arena.top_left;
    let mut vel = Point::new(3, 2);
    let mut ticker = Ticker::every(Duration::from_hz(60));
    let (mut frames, mut busy_us) = (0u32, 0u64);

    loop {
        fill(fb, Rectangle::new(pos, Size::new(BOX, BOX)), Rgb565::BLACK);
        pos += vel;
        let br = arena.bottom_right().unwrap() - Point::new(BOX as i32, BOX as i32);
        if pos.x <= arena.top_left.x || pos.x >= br.x {
            vel.x = -vel.x;
        }
        if pos.y <= arena.top_left.y || pos.y >= br.y {
            vel.y = -vel.y;
        }
        pos = pos.component_max(arena.top_left).component_min(br);
        fill(
            fb,
            Rectangle::new(pos, Size::new(BOX, BOX)),
            Rgb565::CSS_ORANGE,
        );

        let t = Instant::now();
        lcd.flush(fb).await.unwrap();
        busy_us += t.elapsed().as_micros();
        frames += 1;
        if frames == 120 {
            info!(
                "dirty-row flush: {} us/frame avg",
                busy_us / u64::from(frames)
            );
            (frames, busy_us) = (0, 0);
        }
        ticker.next().await;
    }
}

fn fill(fb: &mut display::Frame, r: Rectangle, c: Rgb565) {
    r.into_styled(PrimitiveStyle::with_fill(c))
        .draw(fb)
        .unwrap();
}

fn draw_alignment_test(fb: &mut display::Frame) {
    let size = fb.size();
    fb.clear(Rgb565::BLACK).unwrap();
    Rectangle::new(Point::zero(), size)
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 1))
        .draw(fb)
        .unwrap();

    let (w, h) = (size.width as i32, size.height as i32);
    let corner = Size::new(6, 6);
    for (p, c) in [
        (Point::new(0, 0), Rgb565::RED),
        (Point::new(w - 6, 0), Rgb565::GREEN),
        (Point::new(0, h - 6), Rgb565::BLUE),
        (Point::new(w - 6, h - 6), Rgb565::YELLOW),
    ] {
        fill(fb, Rectangle::new(p, corner), c);
    }

    // Colour bars: also a quick check of RGB/BGR order and inversion.
    let bars = [
        Rgb565::RED,
        Rgb565::GREEN,
        Rgb565::BLUE,
        Rgb565::WHITE,
        Rgb565::CSS_GRAY,
    ];
    for (i, c) in bars.into_iter().enumerate() {
        fill(
            fb,
            Rectangle::new(Point::new(12 + 44 * i as i32, 30), Size::new(40, 36)),
            c,
        );
    }

    let text = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    Text::new("oxstack  240x135  R G B W gray", Point::new(12, 20), text)
        .draw(fb)
        .unwrap();
}
