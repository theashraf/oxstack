//! Scan conversion: depth-tested triangles and lines in screen space.
//!
//! Triangles use integer edge functions on vertices snapped to 1/16 pixel.
//! For each row the covered span is solved exactly (one division per
//! edge), so the inner loop is only "compare depth, maybe write". A tie
//! bias on each edge gives every pixel centre on a shared edge to exactly
//! one of the two triangles: no gaps, no double writes.

use oxstack_gfx::Framebuffer;

use crate::DepthBuffer;

/// Sub-pixel precision: 4 bits, so vertices snap to 1/16 pixel.
const SUB: i32 = 16;
/// Depth is interpolated in fixed point with 8 fractional bits below the
/// stored `u16`, so stepping across a row does not lose precision.
const DEPTH_FRAC: u32 = 8;
/// Interpolated depth stays in `1..=65535` once shifted: 0 is reserved for
/// "nothing drawn", so anything drawn beats a cleared buffer.
const DEPTH_LO: f32 = (1u32 << DEPTH_FRAC) as f32;
const DEPTH_HI: f32 = (65535u32 << DEPTH_FRAC) as f32;

/// Largest supported width or height. Snapped coordinates are clamped to
/// `-dim..2·dim` pixels, and at 512 every edge-function product stays well
/// inside `i32` (worst case ≈ 1.2 × 10⁹).
pub(crate) const MAX_DIM: usize = 512;

/// A projected vertex: pixel coordinates (x right, y down, pixel centres at
/// +0.5) and nearness in `0..=1` (1 at the near plane, 0 at the far plane).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ScreenVertex {
    pub x: f32,
    pub y: f32,
    pub near: f32,
}

/// One edge as a function of the pixel column on row `y`:
/// `e(x, y) = a·x + b·y + c`, inside where `e >= 0`.
#[derive(Clone, Copy)]
struct Edge {
    a: i32,
    b: i32,
    c: i32,
}

impl Edge {
    fn new(xa: i32, ya: i32, xb: i32, yb: i32) -> Self {
        let (dx, dy) = (xb - xa, yb - ya);
        // Inside is the positive side. The edge's direction decides who
        // owns a pixel centre exactly on it: (dx, dy) and (−dx, −dy) never
        // both pass, so a shared edge is drawn exactly once.
        let owns_ties = dy < 0 || (dy == 0 && dx > 0);
        let bias = if owns_ties { 0 } else { -1 };
        // E(P) = dx·(Py − ya) − dy·(Px − xa) at P = (16x + 8, 16y + 8).
        Self {
            a: -SUB * dy,
            b: SUB * dx,
            c: dx * (SUB / 2 - ya) - dy * (SUB / 2 - xa) + bias,
        }
    }

    /// Narrows `lo..=hi` to the columns of row `y` inside this edge.
    #[inline]
    fn clip_row(&self, y: i32, lo: &mut i32, hi: &mut i32) {
        let e = self.b * y + self.c;
        if self.a > 0 {
            // a·x + e >= 0  ⇔  x >= ⌈−e / a⌉
            *lo = (*lo).max(-e.div_euclid(self.a));
        } else if self.a < 0 {
            // a·x + e >= 0  ⇔  x <= ⌊e / −a⌋
            *hi = (*hi).min(e.div_euclid(-self.a));
        } else if e < 0 {
            *hi = *lo - 1;
        }
    }
}

/// A covered run of pixels `x0..=x1` on row `y`, with fixed-point depth
/// `d` at `x0`, changing by `step` per pixel.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Span {
    pub x0: usize,
    pub x1: usize,
    pub d: i32,
    pub step: i32,
}

/// A triangle ready to be scanned: rows `y0..=y1`, three edges and the
/// depth plane.
pub(crate) struct Setup {
    pub y0: usize,
    pub y1: usize,
    x0: i32,
    x1: i32,
    edges: [Edge; 3],
    /// Depth at vertex 0, and its change per pixel in x and y.
    origin: (f32, f32, f32),
    ddx: f32,
    ddy: f32,
}

fn snap(v: f32, dim: usize) -> i32 {
    let lim = (dim as i32 * SUB) as f32;
    // NaN survives the clamp and then converts to 0, which is harmless.
    libm::roundf(v * SUB as f32).clamp(-lim, 2.0 * lim) as i32
}

impl Setup {
    /// `None` for a triangle with no area or entirely off-screen.
    pub fn new<const W: usize, const H: usize>(v: [ScreenVertex; 3]) -> Option<Self> {
        let mut p = v.map(|v| (snap(v.x, W), snap(v.y, H), v.near));
        let area = |p: &[(i32, i32, f32); 3]| {
            (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[1].1 - p[0].1) * (p[2].0 - p[0].0)
        };
        let mut a2 = area(&p);
        if a2 == 0 {
            return None;
        }
        if a2 < 0 {
            p.swap(1, 2);
            a2 = -a2;
        }

        // Rows and columns whose pixel centres can be inside.
        let centre_range = |lo: i32, hi: i32, dim: usize| {
            let first = -((SUB / 2 - lo).div_euclid(SUB)); // ⌈(lo − 8) / 16⌉
            let last = (hi - SUB / 2).div_euclid(SUB); // ⌊(hi − 8) / 16⌋
            (first.max(0), last.min(dim as i32 - 1))
        };
        let (xs, ys) = (p.map(|p| p.0), p.map(|p| p.1));
        let (x0, x1) = centre_range(*xs.iter().min()?, *xs.iter().max()?, W);
        let (y0, y1) = centre_range(*ys.iter().min()?, *ys.iter().max()?, H);
        if x0 > x1 || y0 > y1 {
            return None;
        }

        let edges = [
            Edge::new(p[0].0, p[0].1, p[1].0, p[1].1),
            Edge::new(p[1].0, p[1].1, p[2].0, p[2].1),
            Edge::new(p[2].0, p[2].1, p[0].0, p[0].1),
        ];

        // Depth plane through the *snapped* vertices, in pixels.
        let f = |v: i32| v as f32 * (1.0 / SUB as f32);
        let (x10, y10) = (f(p[1].0 - p[0].0), f(p[1].1 - p[0].1));
        let (x20, y20) = (f(p[2].0 - p[0].0), f(p[2].1 - p[0].1));
        let (d0, d10, d20) = (
            p[0].2 * DEPTH_HI,
            (p[1].2 - p[0].2) * DEPTH_HI,
            (p[2].2 - p[0].2) * DEPTH_HI,
        );
        let inv = crate::math::recip(a2 as f32 * (1.0 / (SUB * SUB) as f32));
        Some(Self {
            y0: y0 as usize,
            y1: y1 as usize,
            x0,
            x1,
            edges,
            origin: (f(p[0].0), f(p[0].1), d0),
            ddx: (d10 * y20 - d20 * y10) * inv,
            ddy: (d20 * x10 - d10 * x20) * inv,
        })
    }

    /// The covered span on row `y` (within `y0..=y1`), if any.
    #[inline]
    pub fn span(&self, y: usize) -> Option<Span> {
        let (mut lo, mut hi) = (self.x0, self.x1);
        for e in &self.edges {
            e.clip_row(y as i32, &mut lo, &mut hi);
        }
        if lo > hi {
            return None;
        }
        // Depth at both ends, clamped, then an integer step between them:
        // every value in the span stays inside the clamped range, even for
        // a sliver whose plane is numerically wild.
        let (ox, oy, od) = self.origin;
        let row = od + self.ddy * (y as f32 + 0.5 - oy);
        let at = |x: i32| (row + self.ddx * (x as f32 + 0.5 - ox)).clamp(DEPTH_LO, DEPTH_HI) as i32;
        let (d_lo, d_hi) = (at(lo), at(hi));
        let n = hi - lo;
        Some(Span {
            x0: lo as usize,
            x1: hi as usize,
            d: d_lo,
            step: if n > 0 { (d_hi - d_lo) / n } else { 0 },
        })
    }
}

/// Fills a triangle with `color` (wire format) where it is nearer than the
/// depth buffer. Returns `false` if nothing could be covered.
pub(crate) fn fill_triangle<const W: usize, const H: usize>(
    fb: &mut Framebuffer<W, H>,
    depth: &mut DepthBuffer<W, H>,
    v: [ScreenVertex; 3],
    color: u16,
) -> bool {
    let Some(s) = Setup::new::<W, H>(v) else {
        return false;
    };
    let rows = s.y0..s.y1 + 1;
    let colors = fb.wire_rows_mut(rows.clone());
    let depths = depth.rows_mut(rows);
    for ((y, crow), drow) in (s.y0..).zip(colors).zip(depths) {
        let Some(sp) = s.span(y) else { continue };
        let mut d = sp.d;
        let px = crow[sp.x0..=sp.x1].iter_mut().zip(&mut drow[sp.x0..=sp.x1]);
        for (c, z) in px {
            let near = (d >> DEPTH_FRAC) as u16;
            if near > *z {
                *z = near;
                *c = color;
            }
            d += sp.step;
        }
    }
    true
}

/// Draws a one-pixel line from `a` to `b`, depth-tested with `>=` so a line
/// drawn on a face it lies in still shows.
pub(crate) fn line<const W: usize, const H: usize>(
    fb: &mut Framebuffer<W, H>,
    depth: &mut DepthBuffer<W, H>,
    a: ScreenVertex,
    b: ScreenVertex,
    color: u16,
) {
    if !(a.x + a.y + b.x + b.y).is_finite() {
        return;
    }
    // Integer Bresenham between the pixels holding each end: no float
    // steps that could land just short of a pixel and skip it. Clipping
    // keeps ends near the screen; the clamp only bounds garbage input.
    let lim = 4.0 * (W + H) as f32;
    let px = |v: f32| libm::floorf(v.clamp(-lim, lim)) as i32;
    let (mut x, mut y, x1, y1) = (px(a.x), px(a.y), px(b.x), px(b.y));
    if y.max(y1) < 0 || y.min(y1) >= H as i32 {
        return;
    }
    let (r0, r1) = (y.min(y1).max(0), y.max(y1).min(H as i32 - 1));
    let colors = fb.wire_rows_mut(r0 as usize..r1 as usize + 1);
    let depths = depth.rows_mut(r0 as usize..r1 as usize + 1);

    let (dx, dy) = ((x1 - x).abs(), -(y1 - y).abs());
    let (sx, sy) = ((x1 - x).signum(), (y1 - y).signum());
    // Each iteration steps once along the major axis.
    let dn = (b.near - a.near) / dx.max(-dy).max(1) as f32;
    let (mut err, mut near) = (dx + dy, a.near);
    loop {
        if (0..W as i32).contains(&x) && (r0..=r1).contains(&y) {
            let (cx, cy) = (x as usize, (y - r0) as usize);
            let n = (near * 65535.0).clamp(1.0, 65535.0) as u16;
            if n >= depths[cy][cx] {
                depths[cy][cx] = n;
                colors[cy][cx] = color;
            }
        }
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
        near += dn;
    }
}

#[cfg(test)]
// Pixel coordinates read more clearly as indices in the oracle code.
#[allow(clippy::needless_range_loop)]
mod tests {
    extern crate std;
    use std::vec::Vec;

    use super::*;
    use embedded_graphics::pixelcolor::Rgb565;
    use embedded_graphics::prelude::*;
    use oxstack_gfx::wire_color;

    const W: usize = 40;
    const H: usize = 30;

    /// A small deterministic PRNG, so failures reproduce.
    struct Rng(u64);
    impl Rng {
        fn f(&mut self) -> f32 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 >> 40) as f32 / (1u64 << 24) as f32
        }
        fn range(&mut self, lo: f32, hi: f32) -> f32 {
            lo + (hi - lo) * self.f()
        }
    }

    fn sv(x: f32, y: f32) -> ScreenVertex {
        ScreenVertex { x, y, near: 0.5 }
    }

    fn coverage(tris: &[[ScreenVertex; 3]]) -> [[u8; W]; H] {
        let mut count = [[0u8; W]; H];
        for &t in tris {
            let Some(s) = Setup::new::<W, H>(t) else {
                continue;
            };
            for y in s.y0..=s.y1 {
                if let Some(sp) = s.span(y) {
                    for c in &mut count[y][sp.x0..=sp.x1] {
                        *c += 1;
                    }
                }
            }
        }
        count
    }

    /// Independent oracle: is the pixel centre strictly inside / on the
    /// convex polygon, using exact f64 maths on the snapped vertices?
    fn classify(poly: &[(f64, f64)], x: usize, y: usize) -> (bool, bool) {
        let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
        let n = poly.len();
        let crosses: Vec<f64> = (0..n)
            .map(|i| {
                let (a, b) = (poly[i], poly[(i + 1) % n]);
                (b.0 - a.0) * (py - a.1) - (b.1 - a.1) * (px - a.0)
            })
            .collect();
        let strict = crosses.iter().all(|&c| c > 0.0) || crosses.iter().all(|&c| c < 0.0);
        let inclusive = crosses.iter().all(|&c| c >= 0.0) || crosses.iter().all(|&c| c <= 0.0);
        (strict, inclusive)
    }

    fn snapped(v: ScreenVertex) -> (f64, f64) {
        let s = |c: f32| libm::roundf(c * 16.0) as f64 / 16.0;
        (s(v.x), s(v.y))
    }

    /// Every pixel strictly inside is covered exactly once; pixels outside
    /// are never covered; nothing is ever covered twice.
    fn check_tiling(poly: &[ScreenVertex], tris: &[[ScreenVertex; 3]]) {
        let outline: Vec<_> = poly.iter().copied().map(snapped).collect();
        let count = coverage(tris);
        for y in 0..H {
            for x in 0..W {
                let (strict, inclusive) = classify(&outline, x, y);
                let c = count[y][x];
                assert!(c <= 1, "pixel ({x}, {y}) drawn {c} times");
                if strict {
                    assert_eq!(c, 1, "gap at ({x}, {y})");
                }
                if !inclusive {
                    assert_eq!(c, 0, "({x}, {y}) is outside");
                }
            }
        }
    }

    #[test]
    fn split_quads_tile_exactly() {
        let mut rng = Rng(1);
        for _ in 0..500 {
            let (cx, cy) = (rng.range(5.0, 35.0), rng.range(5.0, 25.0));
            let (rx, ry) = (rng.range(2.0, 30.0), rng.range(2.0, 20.0));
            let quad: Vec<_> = (0..4)
                .map(|i| {
                    let a = (i as f32 + rng.range(-0.3, 0.3)) * core::f32::consts::FRAC_PI_2;
                    sv(cx + rx * libm::cosf(a), cy + ry * libm::sinf(a))
                })
                .collect();
            let mut tris = [[quad[0], quad[1], quad[2]], [quad[0], quad[2], quad[3]]];
            if rng.f() < 0.5 {
                tris[1].swap(1, 2); // windings may differ
            }
            check_tiling(&quad, &tris);
        }
    }

    #[test]
    fn fans_around_a_shared_vertex_tile_exactly() {
        let mut rng = Rng(2);
        for _ in 0..300 {
            let n = 8 + (rng.f() * 6.0) as usize;
            let (cx, cy) = (rng.range(0.0, 40.0), rng.range(0.0, 30.0));
            let r = rng.range(3.0, 40.0);
            let ring: Vec<_> = (0..n)
                .map(|i| {
                    let a = (i as f32 + rng.range(-0.4, 0.4)) * core::f32::consts::TAU / n as f32;
                    sv(cx + r * libm::cosf(a), cy + r * libm::sinf(a))
                })
                .collect();
            let hub = sv(cx + rng.range(-0.3, 0.3) * r, cy + rng.range(-0.3, 0.3) * r);
            let tris: Vec<_> = (0..n).map(|i| [hub, ring[i], ring[(i + 1) % n]]).collect();
            check_tiling(&ring, &tris);
        }
    }

    #[test]
    fn extreme_and_degenerate_input_never_overflows() {
        // Debug builds panic on integer overflow, so surviving is the test.
        let mut rng = Rng(3);
        let wild = [
            f32::NAN,
            f32::INFINITY,
            -f32::INFINITY,
            1e30,
            -1e30,
            1e5,
            -1e5,
        ];
        for i in 0..20_000 {
            let mut v = [0; 3].map(|_| sv(rng.range(-3000.0, 3000.0), rng.range(-3000.0, 3000.0)));
            match i % 4 {
                0 => v[2] = sv(v[0].x + 1e-3, v[0].y + 2000.0), // sliver
                1 => v[1] = sv(v[0].x + rng.f() * 1e-2, v[0].y + rng.f() * 1e-2),
                2 => v[(i / 4) % 3].x = wild[(i / 12) % wild.len()],
                _ => {}
            }
            for k in 0..3 {
                v[k].near = rng.range(-2.0, 3.0);
            }
            if let Some(s) = Setup::new::<MAX_DIM, MAX_DIM>(v) {
                for y in s.y0..=s.y1 {
                    let _ = s.span(y);
                }
            }
        }
    }

    #[test]
    fn nearer_wins_in_either_order() {
        let draw = |order: [usize; 2]| {
            let mut fb = Framebuffer::<W, H>::new();
            let mut depth = DepthBuffer::<W, H>::new();
            // Flat at 0.5, and a slope from 0.2 (left) to 0.8 (right).
            let flat = [0.5, 0.5, 0.5];
            let slope = [0.2, 0.8, 0.8];
            let tris = [(flat, Rgb565::RED), (slope, Rgb565::BLUE)];
            for i in order {
                let (n, c) = tris[i];
                let v = [
                    ScreenVertex {
                        x: 0.0,
                        y: 0.0,
                        near: n[0],
                    },
                    ScreenVertex {
                        x: 40.0,
                        y: 0.0,
                        near: n[1],
                    },
                    ScreenVertex {
                        x: 40.0,
                        y: 30.0,
                        near: n[2],
                    },
                ];
                // Same shape both times, so the depth values decide.
                assert!(fill_triangle(&mut fb, &mut depth, v, wire_color(c)));
            }
            fb
        };
        let (a, b) = (draw([0, 1]), draw([1, 0]));
        assert_eq!(a.as_bytes(), b.as_bytes());
        assert_eq!(
            a.pixel(5, 1),
            Some(Rgb565::RED),
            "slope is farther on the left"
        );
        assert_eq!(
            a.pixel(35, 1),
            Some(Rgb565::BLUE),
            "and nearer on the right"
        );
    }

    #[test]
    fn depth_is_interpolated_and_never_zero() {
        let mut fb = Framebuffer::<W, H>::new();
        let mut depth = DepthBuffer::<W, H>::new();
        let v = [
            ScreenVertex {
                x: 0.0,
                y: 0.0,
                near: 0.0,
            },
            ScreenVertex {
                x: 40.0,
                y: 0.0,
                near: 1.0,
            },
            ScreenVertex {
                x: 0.0,
                y: 30.0,
                near: 0.0,
            },
        ];
        fill_triangle(&mut fb, &mut depth, v, wire_color(Rgb565::WHITE));
        let (l, r) = (depth.get(0, 0).unwrap(), depth.get(30, 0).unwrap());
        assert!(l >= 1, "far plane still counts as drawn");
        // Pixel centre 30.5 of 40 → nearness ≈ 0.7625.
        assert!((r as f32 / 65535.0 - 0.7625).abs() < 0.002, "{r}");
        assert!(fb.take_dirty().is_some());
    }

    #[test]
    fn lines_are_clipped_to_the_screen() {
        let mut fb = Framebuffer::<W, H>::new();
        let mut depth = DepthBuffer::<W, H>::new();
        let c = wire_color(Rgb565::GREEN);
        line(&mut fb, &mut depth, sv(-10.0, 5.5), sv(100.0, 5.5), c);
        assert!((0..W).all(|x| fb.pixel(x, 5) == Some(Rgb565::GREEN)));
        assert_eq!(fb.pixel(0, 6), Some(Rgb565::BLACK));
        line(&mut fb, &mut depth, sv(-10.0, -10.0), sv(-1.0, 50.0), c);
        line(&mut fb, &mut depth, sv(f32::NAN, 0.0), sv(1.0, 1.0), c);
    }
}
