//! Renders a test scene on the host and writes it as a BMP, to inspect the
//! pipeline without hardware:
//!
//! ```sh
//! cargo run -p oxstack-3d --example render_bmp -- scene.bmp [time_s]
//! ```

use std::time::Instant;

use embedded_graphics::pixelcolor::{Rgb565, RgbColor};
use oxstack_3d::math::{Mat4, Quat, Vec3, radians, sin_cos};
use oxstack_3d::mesh::{self, Face, Mesh};
use oxstack_3d::{Camera, DepthBuffer, Fog, Renderer};
use oxstack_gfx::Framebuffer;

const W: usize = 240;
const H: usize = 135;
const SKY: Rgb565 = Rgb565::new(3, 8, 12);

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| "scene.bmp".into());
    let t: f32 = args
        .next()
        .map_or(1.0, |s| s.parse().expect("time in seconds"));

    let (floor_v, floor_f) = checkerboard(8, 2.0);
    let floor = Mesh::new(&floor_v, &floor_f);

    let mut fb = Box::new(Framebuffer::<W, H>::new());
    let mut depth = Box::new(DepthBuffer::<W, H>::new());

    let (s, c) = sin_cos(0.3 * t);
    let mut camera = Camera::perspective(radians(60.0), 0.3, 40.0);
    camera.position = Vec3::new(7.0 * s, 3.0, 7.0 * c);
    camera.look_at(Vec3::new(0.0, 0.5, 0.0), Vec3::Y);

    let start = Instant::now();
    let mut r = Renderer::new(&mut fb, &mut depth, &camera);
    r.clear(SKY);
    r.fog = Some(Fog {
        color: SKY,
        start: 8.0,
        end: 18.0,
    });
    r.draw_mesh(&floor, &Mat4::IDENTITY);
    let spin = Quat::from_rotation_y(t) * Quat::from_rotation_x(0.6 * t);
    r.draw_mesh(
        &mesh::CUBE,
        &Mat4::from_scale_rotation_translation(Vec3::ONE, spin, Vec3::new(0.0, 1.2, 0.0)),
    );
    r.draw_mesh(
        &mesh::ICOSAHEDRON,
        &Mat4::from_scale_rotation_translation(
            Vec3::splat(0.9),
            spin.inverse(),
            Vec3::new(3.0, 1.0 + 0.3 * sin_cos(2.0 * t).0, -1.0),
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
    // Axes through the origin, over the floor.
    r.draw_line(
        Vec3::new(0.0, 0.01, 0.0),
        Vec3::new(2.5, 0.01, 0.0),
        Rgb565::RED,
    );
    r.draw_line(
        Vec3::new(0.0, 0.01, 0.0),
        Vec3::new(0.0, 2.5, 0.0),
        Rgb565::GREEN,
    );
    r.draw_line(
        Vec3::new(0.0, 0.01, 0.0),
        Vec3::new(0.0, 0.01, 2.5),
        Rgb565::BLUE,
    );
    let stats = r.stats();
    let elapsed = start.elapsed();

    println!("{stats:?}\nhost render time: {elapsed:?}");
    write_bmp(&path, &fb).expect("write BMP");
    println!("wrote {path}");
}

/// An `n`×`n` checkerboard of `size`-unit squares on y = 0, centred.
fn checkerboard(n: u16, size: f32) -> (Vec<Vec3>, Vec<Face>) {
    let half = n as f32 * size / 2.0;
    let mut v = Vec::new();
    for z in 0..=n {
        for x in 0..=n {
            v.push(Vec3::new(
                x as f32 * size - half,
                0.0,
                z as f32 * size - half,
            ));
        }
    }
    let idx = |x: u16, z: u16| z * (n + 1) + x;
    let (light, dark) = (Rgb565::new(10, 44, 12), Rgb565::new(4, 20, 8));
    let mut f = Vec::new();
    for z in 0..n {
        for x in 0..n {
            let c = if (x + z) % 2 == 0 { light } else { dark };
            f.push(Face::new(idx(x, z), idx(x, z + 1), idx(x + 1, z + 1), c));
            f.push(Face::new(idx(x, z), idx(x + 1, z + 1), idx(x + 1, z), c));
        }
    }
    (v, f)
}

fn write_bmp(path: &str, fb: &Framebuffer<W, H>) -> std::io::Result<()> {
    let row_bytes = W * 3; // 720, already a multiple of 4
    let size = 54 + row_bytes * H;
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(W as i32).to_le_bytes());
    out.extend_from_slice(&(H as i32).to_le_bytes()); // positive: bottom-up
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    for y in (0..H).rev() {
        for x in 0..W {
            let p = fb.pixel(x, y).unwrap();
            // Expand 5/6-bit channels to 8 bits.
            let (r, g, b) = (
                p.r() << 3 | p.r() >> 2,
                p.g() << 2 | p.g() >> 4,
                p.b() << 3 | p.b() >> 2,
            );
            out.extend_from_slice(&[b, g, r]);
        }
    }
    std::fs::write(path, out)
}
