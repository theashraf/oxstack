//! Finds how the BMI270 is mounted, in three poses, and prints the
//! `AxisMap` to use as `oxstack_cardputer::imu::BOARD_AXES`.
//!
//! Hold the Cardputer as the screen asks and press **Space** each time:
//!
//! 1. flat on a table, screen up (board +Z up);
//! 2. standing on its bottom (keyboard) edge, screen facing you (+Y up);
//! 3. standing on its left edge, so the right edge is up (+X up).
//!
//! It then shows a spirit level in the measured frame so you can check the
//! result at once. **r** starts over. Raw chip-frame readings are logged
//! throughout so a mistake is easy to spot.
#![no_std]
#![no_main]

use defmt::{info, warn};
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Ticker, Timer};
use embedded_graphics::mono_font::MonoTextStyleBuilder;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, Line, PrimitiveStyle};
use embedded_graphics::text::{Baseline, Text};
use esp_backtrace as _;
use esp_println as _;
use oxstack_cardputer::gfx::text;
use oxstack_cardputer::imu::{self, Axis, AxisMap, Imu};
use oxstack_cardputer::input::Action;
use oxstack_cardputer::keyboard::Keyboard;
use oxstack_cardputer::{self as bsp, display};

esp_bootloader_esp_idf::esp_app_desc!();

const POSES: [(&str, &str); 3] = [
    ("1/3  Lay it FLAT, screen up", "(board +Z points up)"),
    (
        "2/3  Stand on the KEYBOARD edge,",
        "screen facing you (+Y up)",
    ),
    ("3/3  Stand on the LEFT edge,", "right edge up (+X up)"),
];

fn label(fb: &mut display::Frame, y: i32, s: &str, color: Rgb565) {
    let style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(color)
        .background_color(Rgb565::BLACK)
        .build();
    let mut l = text!(40, "{}", s);
    Text::with_baseline(l.pad(40), Point::new(2, y), style, Baseline::Top)
        .draw(fb)
        .unwrap();
}

/// Averages 50 chip-frame samples (half a second).
async fn average(imu: &mut Imu<'_>) -> [f32; 3] {
    let mut sum = [0.0f32; 3];
    for _ in 0..50 {
        let s = imu.read_chip().await.unwrap();
        for (acc, v) in sum.iter_mut().zip(s.accel) {
            *acc += v / 50.0;
        }
        Timer::after_millis(10).await;
    }
    sum
}

async fn wait_space(keyboard: &mut Keyboard<'_>) -> bool {
    loop {
        if let Ok(ev) = keyboard.next_event().await
            && ev.pressed
        {
            match ev.action() {
                Some(Action::Char(' ')) => return true,
                Some(Action::Char('r')) => return false,
                _ => {}
            }
        }
    }
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let mut board = bsp::init(bsp::default_config());
    let _backlight = board.backlight.init();
    let bus = board.sys_i2c.into_bus();
    let mut lcd = board.display.init().await.unwrap();
    let mut keyboard = board.keyboard.init(bus).await.unwrap();
    let mut imu = board.imu.init(bus, imu::Config::default()).await.unwrap();
    let fb = display::framebuffer();

    'again: loop {
        fb.clear(Rgb565::BLACK).unwrap();
        label(fb, 0, "oxstack IMU axis finder", Rgb565::CSS_ORANGE);
        let mut readings = [[0.0f32; 3]; 3];
        for (i, (what, detail)) in POSES.iter().enumerate() {
            label(fb, 20, what, Rgb565::WHITE);
            label(fb, 30, detail, Rgb565::WHITE);
            label(fb, 50, "then press Space", Rgb565::GREEN);
            lcd.flush(fb).await.unwrap();
            if !wait_space(&mut keyboard).await {
                continue 'again;
            }
            label(fb, 50, "measuring...", Rgb565::CSS_GRAY);
            lcd.flush(fb).await.unwrap();
            let a = average(&mut imu).await;
            info!(
                "pose {}: chip accel = [{}, {}, {}] g",
                i + 1,
                a[0],
                a[1],
                a[2]
            );
            readings[i] = a;
            label(
                fb,
                60 + 10 * i as i32,
                &text!(40, "{:+.2} {:+.2} {:+.2}", a[0], a[1], a[2]),
                Rgb565::CSS_GRAY,
            );
        }
        let [z_up, y_up, x_up] = readings;
        let map = match AxisMap::from_gravity(x_up, y_up, z_up) {
            Ok(map) => map,
            Err(e) => {
                warn!("could not determine axes: {}", e);
                label(
                    fb,
                    100,
                    &text!(40, "Unclear: {:?}. r = retry", e),
                    Rgb565::RED,
                );
                lcd.flush(fb).await.unwrap();
                while wait_space(&mut keyboard).await {}
                continue 'again;
            }
        };
        let handed = if map.is_right_handed() {
            "rotation"
        } else {
            "MIRROR (one pose wrong?)"
        };
        info!(
            "BOARD_AXES = AxisMap {{ x: {}, y: {}, z: {} }}  // {}",
            map.x.source(),
            map.y.source(),
            map.z.source(),
            handed
        );
        info!(
            "current default: {} {} {}",
            imu::BOARD_AXES.x.source(),
            imu::BOARD_AXES.y.source(),
            imu::BOARD_AXES.z.source()
        );
        imu.set_axes(map);

        // Spirit level in the measured frame; `r` restarts.
        fb.clear(Rgb565::BLACK).unwrap();
        label(
            fb,
            0,
            &text!(
                40,
                "x:{} y:{} z:{}",
                short(map.x),
                short(map.y),
                short(map.z)
            ),
            Rgb565::CSS_ORANGE,
        );
        label(fb, 10, &text!(40, "{}   r = redo", handed), Rgb565::WHITE);
        let centre = Point::new(display::WIDTH as i32 / 2, 80);
        let radius: i32 = 40;
        let mut ticker = Ticker::every(Duration::from_millis(50));
        loop {
            match select(ticker.next(), keyboard.next_event()).await {
                Either::First(()) => {}
                Either::Second(Ok(ev)) if ev.pressed && ev.action() == Some(Action::Char('r')) => {
                    continue 'again;
                }
                Either::Second(_) => continue,
            }
            let s = imu.read().await.unwrap();
            let [x, y, _z] = s.accel;
            // Erase, then draw: ring, cross-hair, bubble. Tilting the right
            // edge down (+X down) moves the bubble left, like a real level.
            Circle::with_center(centre, (2 * radius + 12) as u32)
                .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
                .draw(fb)
                .unwrap();
            Circle::with_center(centre, 2 * radius as u32)
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::CSS_GRAY, 1))
                .draw(fb)
                .unwrap();
            Line::new(
                centre - Point::new(radius, 0),
                centre + Point::new(radius, 0),
            )
            .into_styled(PrimitiveStyle::with_stroke(Rgb565::CSS_DARK_SLATE_GRAY, 1))
            .draw(fb)
            .unwrap();
            Line::new(
                centre - Point::new(0, radius),
                centre + Point::new(0, radius),
            )
            .into_styled(PrimitiveStyle::with_stroke(Rgb565::CSS_DARK_SLATE_GRAY, 1))
            .draw(fb)
            .unwrap();
            let bubble = centre
                + Point::new(
                    (-x.clamp(-1.0, 1.0) * radius as f32) as i32,
                    (y.clamp(-1.0, 1.0) * radius as f32) as i32,
                );
            let level = x.abs() < 0.03 && y.abs() < 0.03;
            Circle::with_center(bubble, 10)
                .into_styled(PrimitiveStyle::with_fill(if level {
                    Rgb565::GREEN
                } else {
                    Rgb565::CSS_ORANGE
                }))
                .draw(fb)
                .unwrap();
            label(
                fb,
                124,
                &text!(40, "x {:+.2}  y {:+.2}  z {:+.2}", x, y, s.accel[2]),
                Rgb565::WHITE,
            );
            lcd.flush(fb).await.unwrap();
        }
    }
}

fn short(a: Axis) -> &'static str {
    match a {
        Axis::PosX => "+X",
        Axis::NegX => "-X",
        Axis::PosY => "+Y",
        Axis::NegY => "-Y",
        Axis::PosZ => "+Z",
        Axis::NegZ => "-Z",
    }
}
