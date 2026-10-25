//! Homogeneous clipping (Sutherland–Hodgman in clip space).
//!
//! Triangles are always clipped exactly against the near and far planes
//! (behind-the-camera geometry would otherwise project inverted, and depth
//! would leave its range). Left/right/top/bottom use a **guard band**: a
//! triangle is only cut if it reaches more than [`GUARD`]× the visible
//! half-width off-screen. Anything closer is handed to the rasteriser,
//! which clips to the screen per row at no cost. Most triangles that
//! cross a screen edge are therefore never cut.

use crate::math::Vec4;

/// Guard-band size in NDC units. The rasteriser's fixed-point range covers
/// `−1..2` screens, and the guard band, at `−0.5..1.5` screens, sits inside it.
pub(crate) const GUARD: f32 = 2.0;

/// Clip-space planes, inside where `plane · v >= 0`.
const PLANES: [Vec4; 6] = [
    Vec4::new(0.0, 0.0, 1.0, 0.0),    // near: z >= 0
    Vec4::new(0.0, 0.0, -1.0, 1.0),   // far: z <= w
    Vec4::new(1.0, 0.0, 0.0, GUARD),  // x >= −G·w
    Vec4::new(-1.0, 0.0, 0.0, GUARD), // x <= G·w
    Vec4::new(0.0, 1.0, 0.0, GUARD),  // y >= −G·w
    Vec4::new(0.0, -1.0, 0.0, GUARD), // y <= G·w
];

/// A triangle can gain one vertex per plane.
pub(crate) const MAX_VERTS: usize = 3 + PLANES.len();

/// Bit `i` set: outside the exact view volume on plane `i` (near, far,
/// left, right, bottom, top). A primitive whose vertices share a bit is
/// invisible.
#[inline]
pub(crate) fn frustum_code(v: Vec4) -> u8 {
    (v.z < 0.0) as u8
        | ((v.z > v.w) as u8) << 1
        | ((v.x < -v.w) as u8) << 2
        | ((v.x > v.w) as u8) << 3
        | ((v.y < -v.w) as u8) << 4
        | ((v.y > v.w) as u8) << 5
}

/// Like [`frustum_code`], against the guard band: a primitive whose codes
/// are all zero can go straight to the rasteriser.
#[inline]
pub(crate) fn guard_code(v: Vec4) -> u8 {
    let g = GUARD * v.w;
    (v.z < 0.0) as u8
        | ((v.z > v.w) as u8) << 1
        | ((v.x < -g) as u8) << 2
        | ((v.x > g) as u8) << 3
        | ((v.y < -g) as u8) << 4
        | ((v.y > g) as u8) << 5
}

/// The point where edge `inside → outside` crosses a plane. Always
/// computed from the inside end, so two triangles sharing the edge get the
/// bit-identical point and no crack opens between them.
#[inline]
fn intersect(inside: Vec4, d_in: f32, outside: Vec4, d_out: f32) -> Vec4 {
    inside + (outside - inside) * (d_in / (d_in - d_out))
}

/// Clips the convex polygon `poly[..n]` against the planes in `planes`
/// (a [`guard_code`] mask), in place. Returns the new vertex count; below
/// 3 means nothing is left.
pub(crate) fn polygon(poly: &mut [Vec4; MAX_VERTS], mut n: usize, planes: u8) -> usize {
    let mut tmp = [Vec4::ZERO; MAX_VERTS];
    for (i, plane) in PLANES.iter().enumerate() {
        if planes & (1 << i) == 0 {
            continue;
        }
        let mut m = 0;
        for k in 0..n {
            let (a, b) = (poly[k], poly[(k + 1) % n]);
            let (da, db) = (plane.dot(a), plane.dot(b));
            if da >= 0.0 {
                tmp[m] = a;
                m += 1;
            }
            if (da >= 0.0) != (db >= 0.0) {
                tmp[m] = if da >= 0.0 {
                    intersect(a, da, b, db)
                } else {
                    intersect(b, db, a, da)
                };
                m += 1;
            }
        }
        n = m;
        poly[..n].copy_from_slice(&tmp[..n]);
        if n < 3 {
            return n;
        }
    }
    n
}

/// Clips segment `a → b` against the planes in `planes`. `None` if nothing
/// is left.
pub(crate) fn segment(mut a: Vec4, mut b: Vec4, planes: u8) -> Option<(Vec4, Vec4)> {
    for (i, plane) in PLANES.iter().enumerate() {
        if planes & (1 << i) == 0 {
            continue;
        }
        let (da, db) = (plane.dot(a), plane.dot(b));
        match (da >= 0.0, db >= 0.0) {
            (true, true) => {}
            (false, false) => return None,
            (true, false) => b = intersect(a, da, b, db),
            (false, true) => a = intersect(b, db, a, da),
        }
    }
    Some((a, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes() {
        let inside = Vec4::new(0.5, -0.5, 0.5, 1.0);
        assert_eq!(frustum_code(inside), 0);
        let offscreen = Vec4::new(1.5, 0.0, 0.5, 1.0);
        assert_eq!(frustum_code(offscreen), 1 << 3);
        assert_eq!(guard_code(offscreen), 0, "inside the guard band");
        assert_eq!(guard_code(Vec4::new(0.0, 0.0, -0.1, 1.0)), 1, "behind near");
    }

    #[test]
    fn near_clip_cuts_a_triangle_into_a_quad() {
        let mut poly = [Vec4::ZERO; MAX_VERTS];
        poly[..3].copy_from_slice(&[
            Vec4::new(0.0, 0.0, -1.0, 1.0), // behind
            Vec4::new(1.0, 0.0, 1.0, 1.0),
            Vec4::new(-1.0, 0.0, 1.0, 1.0),
        ]);
        let n = polygon(&mut poly, 3, 1);
        assert_eq!(n, 4);
        assert!(poly[..n].iter().all(|v| v.z >= 0.0));
        assert!(poly[..n].iter().filter(|v| v.z == 0.0).count() == 2);
    }

    #[test]
    fn shared_edge_clips_to_identical_points() {
        let (a, b) = (
            Vec4::new(0.3, 0.1, -2.0, 1.7),
            Vec4::new(-0.2, 0.4, 3.1, 4.3),
        );
        let (c, d) = (
            Vec4::new(1.0, 1.0, 1.0, 2.0),
            Vec4::new(-1.0, 1.0, 1.0, 2.0),
        );
        let mut t1 = [Vec4::ZERO; MAX_VERTS];
        let mut t2 = [Vec4::ZERO; MAX_VERTS];
        t1[..3].copy_from_slice(&[a, b, c]);
        t2[..3].copy_from_slice(&[b, a, d]); // edge walked the other way
        let (n1, n2) = (polygon(&mut t1, 3, 1), polygon(&mut t2, 3, 1));
        let cut = intersect(b, b.z, a, a.z);
        assert!(t1[..n1].contains(&cut), "{:?}", &t1[..n1]);
        assert!(t2[..n2].contains(&cut), "{:?}", &t2[..n2]);
    }

    #[test]
    fn segments() {
        let a = Vec4::new(0.0, 0.0, -1.0, 1.0);
        let b = Vec4::new(0.0, 0.0, 1.0, 1.0);
        let (p, q) = segment(a, b, 1).unwrap();
        assert_eq!((p.z, q), (0.0, b));
        assert!(segment(a, a, 1).is_none());
    }
}
