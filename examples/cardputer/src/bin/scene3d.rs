//! Software 3D: a checkerboard floor, three flat-shaded solids, depth-tested
//! axis lines and distance fog, seen from an orbiting camera.
//!
//! Every frame is drawn from scratch into the framebuffer and a 16-bit depth
//! buffer, then flushed in full. Every 120 frames it logs the frame rate and
//! render and flush times separately: render is the CPU's work, and flush
//! is the 13 ms DMA transfer to the panel.
#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::Instant;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text};
use esp_backtrace as _;
use esp_println as _;
use oxstack_3d::math::{Mat4, Quat, Vec3, radians, sin_cos};
use oxstack_3d::mesh::{self, Face, Mesh};
use oxstack_3d::{Camera, DepthBuffer, Fog, Renderer};
use oxstack_cardputer::gfx::text;
use oxstack_cardputer::{self as bsp, display};
use static_cell::ConstStaticCell;

esp_bootloader_esp_idf::esp_app_desc!();

const SKY: Rgb565 = Rgb565::new(3, 8, 12);

/// Floor: `N`×`N` squares of `SIZE` units, built at compile time into flash.
const N: usize = 8;
const SIZE: f32 = 2.0;
const FLOOR_VERTS: [Vec3; (N + 1) * (N + 1)] = {
    let mut v = [Vec3::ZERO; (N + 1) * (N + 1)];
    let half = N as f32 * SIZE / 2.0;
    let mut i = 0;
    while i < v.len() {
        let (x, z) = ((i % (N + 1)) as f32, (i / (N + 1)) as f32);
        v[i] = Vec3::new(x * SIZE - half, 0.0, z * SIZE - half);
        i += 1;
    }
    v
};
const FLOOR_FACES: [Face; N * N * 2] = {
    let (light, dark) = (Rgb565::new(10, 44, 12), Rgb565::new(4, 20, 8));
    let mut f = [Face::new(0, 0, 0, light); N * N * 2];
    let mut i = 0;
    while i < N * N {
        let (x, z) = ((i % N) as u16, (i / N) as u16);
        let c = if (x + z) % 2 == 0 { light } else { dark };
        f[2 * i] = Face::new(at(x, z), at(x, z + 1), at(x + 1, z + 1), c);
        f[2 * i + 1] = Face::new(at(x, z), at(x + 1, z + 1), at(x + 1, z), c);
        i += 1;
    }
    f
};
const fn at(x: u16, z: u16) -> u16 {
    z * (N as u16 + 1) + x
}
const FLOOR: Mesh<'static> = Mesh::new(&FLOOR_VERTS, &FLOOR_FACES);

/// 64.8 KB of `.bss`, next to the framebuffer.
static DEPTH: ConstStaticCell<DepthBuffer<{ display::WIDTH }, { display::HEIGHT }>> =
    ConstStaticCell::new(DepthBuffer::new());

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let mut board = bsp::init(bsp::default_config());
    let _backlight = board.backlight.init();
    let mut lcd = board.display.init().await.unwrap();
    let fb = display::framebuffer();
    let depth = DEPTH.take();

    let mut camera = Camera::perspective(radians(60.0), 0.3, 40.0);
    let hud = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let start = Instant::now();
    let (mut frames, mut render_us, mut flush_us) = (0u32, 0u64, 0u64);
    let mut window = Instant::now();
    let mut fps = 0u32;

    loop {
        // Wrapped hourly so angles stay small and `f32` stays precise.
        let t = (start.elapsed().as_micros() % 3_600_000_000) as f32 * 1e-6;
        let (s, c) = sin_cos(0.3 * t);
        camera.position = Vec3::new(7.0 * s, 3.0, 7.0 * c);
        camera.look_at(Vec3::new(0.0, 0.5, 0.0), Vec3::Y);

        let t0 = Instant::now();
        let mut r = Renderer::new(fb, depth, &camera);
        r.clear(SKY);
        r.fog = Some(Fog {
            color: SKY,
            start: 8.0,
            end: 18.0,
        });
        r.draw_mesh(&FLOOR, &Mat4::IDENTITY);
        let spin = Quat::from_rotation_y(t) * Quat::from_rotation_x(0.6 * t);
        r.draw_mesh(
            &mesh::CUBE,
            &Mat4::from_scale_rotation_translation(Vec3::ONE, spin, Vec3::new(0.0, 1.2, 0.0)),
        );
        let bob = 0.3 * sin_cos(2.0 * t).0;
        r.draw_mesh(
            &mesh::ICOSAHEDRON,
            &Mat4::from_scale_rotation_translation(
                Vec3::splat(0.9),
                spin.inverse(),
                Vec3::new(3.0, 1.0 + bob, -1.0),
            ),
        );
        r.draw_mesh(
            &mesh::OCTAHEDRON,
            &Mat4::from_scale_rotation_translation(
                Vec3::new(0.7, 1.2, 0.7),
                Quat::from_rotation_y(-2.0 * t),
                Vec3::new(-3.0, 1.3, 1.0),
            ),
        );
        let o = Vec3::new(0.0, 0.01, 0.0);
        r.draw_line(o, o + Vec3::X * 2.5, Rgb565::RED);
        r.draw_line(o, o + Vec3::Y * 2.5, Rgb565::GREEN);
        r.draw_line(o, o + Vec3::Z * 2.5, Rgb565::BLUE);
        let stats = r.stats();
        let render = t0.elapsed().as_micros();

        Text::with_baseline(
            &text!(32, "{} fps  3D {} ms", fps, render / 1000),
            Point::new(2, 1),
            hud,
            Baseline::Top,
        )
        .draw(r.framebuffer())
        .unwrap();
        // `r` is done: its borrow of `fb` ends here.

        let t1 = Instant::now();
        lcd.flush(fb).await.unwrap();
        flush_us += t1.elapsed().as_micros();
        render_us += render;
        frames += 1;

        if frames == 120 {
            let secs_x100 = window.elapsed().as_millis() / 10;
            fps = (frames as u64 * 100 / secs_x100.max(1)) as u32;
            info!(
                "{} fps | render {} us | flush {} us | {}",
                fps,
                render_us / frames as u64,
                flush_us / frames as u64,
                stats
            );
            (frames, render_us, flush_us) = (0, 0, 0);
            window = Instant::now();
        }
    }
}
