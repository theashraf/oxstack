use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};

use crate::math::{Vec3, recip};

/// Flat (per-face) lighting: an ambient term plus one directional light,
/// like the sun.
///
/// A face's brightness is `ambient + intensity · max(0, n · direction)`,
/// where `n` is its unit normal. Brightness above 1 saturates each colour
/// channel rather than shifting the hue much.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Lighting {
    /// Light that reaches every face, `0..=1`.
    pub ambient: f32,
    /// Direction *toward* the light, in world space. Need not be unit
    /// length.
    pub direction: Vec3,
    /// Strength of the directional light.
    pub intensity: f32,
}

impl Lighting {
    /// Every face at its base colour.
    pub const UNLIT: Self = Self {
        ambient: 1.0,
        direction: Vec3::Y,
        intensity: 0.0,
    };

    /// Ambient 0.3 and a light from above, front and right: every side
    /// of a cube viewed from the front gets a different shade.
    pub const DEFAULT: Self = Self {
        ambient: 0.3,
        direction: Vec3::new(0.4, 1.0, 0.7),
        intensity: 0.8,
    };
}

impl Default for Lighting {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Distance fog: faces fade to `color` between `start` and `end` (world
/// units from the camera). Cheap depth cueing, and it hides the far plane.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Fog {
    /// Colour at and beyond `end`; usually the clear colour.
    pub color: Rgb565,
    /// Distance where fog begins.
    pub start: f32,
    /// Distance where fog is total.
    pub end: f32,
}

impl Fog {
    /// Fog amount at `distance`, `0..=1`.
    pub(crate) fn amount(&self, distance: f32) -> f32 {
        ((distance - self.start) * recip(self.end - self.start)).clamp(0.0, 1.0)
    }
}

/// `base` scaled by `light` (1.0 = unchanged), then blended toward `fog`
/// by `fog_amount`. Integer maths per channel; once per face.
pub(crate) fn shade(base: Rgb565, light: f32, fog: Rgb565, fog_amount: f32) -> Rgb565 {
    // 8.8 fixed point; NaN converts to 0.
    let l = (light * 256.0).clamp(0.0, 1024.0) as u32;
    let f = (fog_amount * 256.0).clamp(0.0, 256.0) as u32;
    let ch = |c: u8, fc: u8, max: u32| {
        let lit = ((c as u32 * l + 128) >> 8).min(max);
        ((lit * (256 - f) + fc as u32 * f + 128) >> 8) as u8
    };
    Rgb565::new(
        ch(base.r(), fog.r(), 31),
        ch(base.g(), fog.g(), 63),
        ch(base.b(), fog.b(), 31),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shade_scales_and_fogs() {
        let c = Rgb565::new(20, 40, 10);
        assert_eq!(shade(c, 1.0, Rgb565::BLACK, 0.0), c);
        assert_eq!(shade(c, 0.5, Rgb565::BLACK, 0.0), Rgb565::new(10, 20, 5));
        assert_eq!(
            shade(c, 2.0, Rgb565::BLACK, 0.0),
            Rgb565::new(31, 63, 20),
            "saturates"
        );
        assert_eq!(shade(c, 1.0, Rgb565::WHITE, 1.0), Rgb565::WHITE);
        assert_eq!(shade(c, f32::NAN, Rgb565::BLACK, f32::NAN), Rgb565::BLACK);
        let fog = Fog {
            color: Rgb565::WHITE,
            start: 10.0,
            end: 20.0,
        };
        assert_eq!((fog.amount(5.0), fog.amount(99.0)), (0.0, 1.0));
        assert!((fog.amount(15.0) - 0.5).abs() < 1e-6);
    }
}
