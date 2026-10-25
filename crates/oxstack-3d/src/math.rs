//! Vectors, matrices and quaternions in `f32`.
//!
//! The ESP32-S3's FPU is single-precision only, so everything here is
//! `f32`. Its hardware does multiply and add, but the compiler turns `f32`
//! division into a ROM software routine, and `libm`'s `sinf`/`cosf`
//! compute in software `f64`. Hot paths therefore use [`recip`],
//! [`rsqrt`] and [`sin_cos`] from this module: they are built only from
//! multiplies and adds, and accurate to a few units in the last place.

mod mat4;
mod quat;
mod vec;

pub use mat4::Mat4;
pub use quat::Quat;
pub use vec::{Vec2, Vec3, Vec4};

pub use core::f32::consts::{FRAC_PI_2, PI, TAU};

/// Square root (correctly rounded).
#[inline]
pub fn sqrt(x: f32) -> f32 {
    libm::sqrtf(x)
}

/// `1 / x` without a division, within 2 ulp, for finite `x` with
/// `1e-37 < |x| < 1e37`.
///
/// A bit-level first guess (about 4 correct bits), then three
/// Newton–Raphson steps `r ← r·(2 − x·r)`, each doubling the correct bits.
#[inline]
pub fn recip(x: f32) -> f32 {
    let mut r = f32::from_bits(0x7EF3_11C3u32.wrapping_sub(x.to_bits()));
    r = r * (2.0 - x * r);
    r = r * (2.0 - x * r);
    r * (2.0 - x * r)
}

/// `1 / √x` without a square root or division, within 2 ulp, for normal
/// `x > 0`. What normalising a vector needs.
#[inline]
pub fn rsqrt(x: f32) -> f32 {
    // The well-known first guess (≈0.2% error), then Newton–Raphson.
    let mut y = f32::from_bits(0x5F37_5A86u32.wrapping_sub(x.to_bits() >> 1));
    let half = 0.5 * x;
    y = y * (1.5 - half * y * y);
    y = y * (1.5 - half * y * y);
    y * (1.5 - half * y * y)
}

/// `(sin(x), cos(x))`, computed together in `f32`: within about `1e-7`
/// for `|x| < 10⁴`, and always on the unit circle. Precision falls off for
/// larger angles, as `f32` itself can no longer place them within a turn;
/// keep angles wrapped. `±inf` and NaN give NaN.
pub fn sin_cos(x: f32) -> (f32, f32) {
    // Past 1e8 the quadrant count below loses exactness and then overflows
    // `i32`. `%` is IEEE fmod, exact, so the reduced angle stays bounded
    // (if not meaningful) and the result stays a valid rotation.
    let x = if x.abs() > 1e8 { x % TAU } else { x };
    // Quadrant q = round(x / (π/2)), remainder r in [−π/4, π/4]. π/2 is
    // split (Cody–Waite) so that q·HI is exact: HI has 8 significant bits.
    const PIO2_HI: f32 = 1.570_312_5;
    const PIO2_LO: f32 = 4.838_268e-4;
    let qf = x * core::f32::consts::FRAC_2_PI;
    let q = if qf >= 0.0 { qf + 0.5 } else { qf - 0.5 } as i32; // NaN → 0
    let r = (x - q as f32 * PIO2_HI) - q as f32 * PIO2_LO;
    // Taylor series, Horner form. At |r| = π/4 the first omitted terms
    // are 2e-9 (sin) and 2.5e-8 (cos), under f32 rounding.
    let r2 = r * r;
    let s = r
        * (1.0
            + r2 * (-1.0 / 6.0
                + r2 * (1.0 / 120.0 + r2 * (-1.0 / 5040.0 + r2 * (1.0 / 362_880.0)))));
    let c = 1.0 + r2 * (-0.5 + r2 * (1.0 / 24.0 + r2 * (-1.0 / 720.0 + r2 * (1.0 / 40_320.0))));
    match q & 3 {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    }
}

/// Tangent, from [`sin_cos`].
#[inline]
pub fn tan(x: f32) -> f32 {
    let (s, c) = sin_cos(x);
    s / c
}

/// Arc cosine; `x` is clamped to `-1..=1` first.
#[inline]
pub fn acos(x: f32) -> f32 {
    libm::acosf(x.clamp(-1.0, 1.0))
}

/// Arc tangent of `y / x`, in the right quadrant.
#[inline]
pub fn atan2(y: f32, x: f32) -> f32 {
    libm::atan2f(y, x)
}

/// Degrees to radians.
#[inline]
pub const fn radians(degrees: f32) -> f32 {
    degrees * (PI / 180.0)
}

#[cfg(test)]
pub(crate) trait Close: Copy + core::fmt::Debug {
    fn max_diff(self, o: Self) -> f32;
}

#[cfg(test)]
impl Close for Vec3 {
    fn max_diff(self, o: Self) -> f32 {
        let d = (self - o).abs();
        d.x.max(d.y).max(d.z)
    }
}

#[cfg(test)]
impl Close for Vec4 {
    fn max_diff(self, o: Self) -> f32 {
        let d = (self - o).abs();
        d.x.max(d.y).max(d.z).max(d.w)
    }
}

#[cfg(test)]
#[track_caller]
pub(crate) fn assert_close<T: Close>(a: T, b: T) {
    assert!(a.max_diff(b) < 1e-5, "{a:?} != {b:?}");
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    /// Deterministic log-uniform magnitudes of both signs.
    fn samples(lo_exp: f32, hi_exp: f32) -> impl Iterator<Item = f32> {
        let mut s = 7u64;
        (0..200_000).map(move |i| {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let u = (s >> 40) as f32 / (1u64 << 24) as f32;
            let m = 10f32.powf(lo_exp + (hi_exp - lo_exp) * u);
            if i % 2 == 0 { m } else { -m }
        })
    }

    #[test]
    fn recip_matches_division() {
        let worst = samples(-36.0, 36.0)
            .map(|x| ((recip(x) as f64 - 1.0 / x as f64) * x as f64).abs())
            .fold(0.0, f64::max);
        assert!(worst < 2.5e-7, "max relative error {worst:e}");
        assert!(
            (recip(4.0) - 0.25).abs() <= f32::EPSILON * 0.25,
            "within an ulp, not exact"
        );
    }

    #[test]
    fn rsqrt_matches_sqrt() {
        let worst = samples(-36.0, 36.0)
            .map(f32::abs)
            .map(|x| ((rsqrt(x) as f64) * (x as f64).sqrt() - 1.0).abs())
            .fold(0.0, f64::max);
        assert!(worst < 2.5e-7, "max relative error {worst:e}");
    }

    #[test]
    fn sin_cos_accuracy() {
        let mut worst = 0f64;
        for i in -2_000_000..=2_000_000 {
            let x = i as f32 * 0.005; // −10⁴..10⁴
            let (s, c) = sin_cos(x);
            let xd = x as f64;
            worst = worst
                .max((s as f64 - xd.sin()).abs())
                .max((c as f64 - xd.cos()).abs());
        }
        assert!(worst < 1.5e-6, "max error {worst:e}");
        let (s, c) = sin_cos(0.5);
        assert!((s as f64 - 0.5f64.sin()).abs() < 1e-7 && (c as f64 - 0.5f64.cos()).abs() < 1e-7);
        assert_eq!(sin_cos(0.0), (0.0, 1.0));
        assert!(sin_cos(f32::NAN).0.is_nan());
    }

    #[test]
    fn sin_cos_stays_on_the_unit_circle_for_any_input() {
        // Angles this large are wrong anyway, but a result off the unit
        // circle would silently de-normalise quaternions built from it.
        for x in [1e5f32, 1e6, 1e7, 3e7, 1e8, 1e10, 3.5e9, 1e30, f32::MAX] {
            for x in [x, -x] {
                let (s, c) = sin_cos(x);
                assert!(
                    s.abs() <= 1.0 + 1e-6 && c.abs() <= 1.0 + 1e-6,
                    "{x}: ({s}, {c})"
                );
                assert!((s * s + c * c - 1.0).abs() < 1e-5, "{x}: ({s}, {c})");
            }
        }
        for x in [f32::INFINITY, f32::NEG_INFINITY] {
            assert!(sin_cos(x).0.is_nan(), "like libm: sin(±inf) is NaN");
        }
    }
}
