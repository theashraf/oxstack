use core::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Sub, SubAssign};

use super::{rsqrt, sqrt};

/// Implements component-wise arithmetic for a vector type.
macro_rules! vec_ops {
    ($T:ident { $($c:ident),+ }) => {
        impl Add for $T {
            type Output = Self;
            #[inline]
            fn add(self, o: Self) -> Self { Self { $($c: self.$c + o.$c),+ } }
        }
        impl Sub for $T {
            type Output = Self;
            #[inline]
            fn sub(self, o: Self) -> Self { Self { $($c: self.$c - o.$c),+ } }
        }
        /// Component-wise product.
        impl Mul for $T {
            type Output = Self;
            #[inline]
            fn mul(self, o: Self) -> Self { Self { $($c: self.$c * o.$c),+ } }
        }
        impl Mul<f32> for $T {
            type Output = Self;
            #[inline]
            fn mul(self, s: f32) -> Self { Self { $($c: self.$c * s),+ } }
        }
        impl Mul<$T> for f32 {
            type Output = $T;
            #[inline]
            fn mul(self, v: $T) -> $T { v * self }
        }
        impl Div<f32> for $T {
            type Output = Self;
            /// One division, then multiplies: division is the slow operation.
            #[inline]
            fn div(self, s: f32) -> Self { self * (1.0 / s) }
        }
        impl Neg for $T {
            type Output = Self;
            #[inline]
            fn neg(self) -> Self { Self { $($c: -self.$c),+ } }
        }
        impl AddAssign for $T {
            #[inline]
            fn add_assign(&mut self, o: Self) { *self = *self + o; }
        }
        impl SubAssign for $T {
            #[inline]
            fn sub_assign(&mut self, o: Self) { *self = *self - o; }
        }
        impl MulAssign<f32> for $T {
            #[inline]
            fn mul_assign(&mut self, s: f32) { *self = *self * s; }
        }

        impl $T {
            /// Every component set to `v`.
            #[inline]
            pub const fn splat(v: f32) -> Self { Self { $($c: v),+ } }

            /// Dot product.
            #[inline]
            pub fn dot(self, o: Self) -> f32 { 0.0 $(+ self.$c * o.$c)+ }

            /// Squared length; no square root.
            #[inline]
            pub fn length_squared(self) -> f32 { self.dot(self) }

            /// Length.
            #[inline]
            pub fn length(self) -> f32 { sqrt(self.length_squared()) }

            /// Unit vector in the same direction (within 2 ulp), or zero for
            /// a zero vector. Uses [`rsqrt`](super::rsqrt): no division.
            #[inline]
            pub fn normalize(self) -> Self {
                let len_sq = self.length_squared();
                if len_sq > 0.0 { self * rsqrt(len_sq) } else { Self::splat(0.0) }
            }

            /// Linear interpolation: `self` at `t = 0`, `to` at `t = 1`.
            #[inline]
            pub fn lerp(self, to: Self, t: f32) -> Self { self + (to - self) * t }

            /// Component-wise minimum.
            #[inline]
            pub fn min(self, o: Self) -> Self { Self { $($c: self.$c.min(o.$c)),+ } }

            /// Component-wise maximum.
            #[inline]
            pub fn max(self, o: Self) -> Self { Self { $($c: self.$c.max(o.$c)),+ } }

            /// Component-wise absolute value.
            #[inline]
            pub fn abs(self) -> Self { Self { $($c: self.$c.abs()),+ } }
        }
    };
}

/// A 2D vector.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
#[allow(missing_docs)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

/// A 3D vector: a point or direction.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
#[allow(missing_docs)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// A 4D vector, mostly a homogeneous (clip-space) position.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
#[allow(missing_docs)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

vec_ops!(Vec2 { x, y });
vec_ops!(Vec3 { x, y, z });
vec_ops!(Vec4 { x, y, z, w });

impl Vec2 {
    #[allow(missing_docs)]
    pub const ZERO: Self = Self::splat(0.0);

    #[allow(missing_docs)]
    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// The 2D cross product `x₁y₂ − y₁x₂`: twice the signed area of the
    /// triangle (0, self, o).
    #[inline]
    pub fn perp_dot(self, o: Self) -> f32 {
        self.x * o.y - self.y * o.x
    }
}

impl Vec3 {
    #[allow(missing_docs)]
    pub const ZERO: Self = Self::splat(0.0);
    #[allow(missing_docs)]
    pub const ONE: Self = Self::splat(1.0);
    /// +X, right.
    pub const X: Self = Self::new(1.0, 0.0, 0.0);
    /// +Y, up.
    pub const Y: Self = Self::new(0.0, 1.0, 0.0);
    /// +Z, toward a default camera (which looks along −Z).
    pub const Z: Self = Self::new(0.0, 0.0, 1.0);

    #[allow(missing_docs)]
    #[inline]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Cross product (right-handed).
    #[inline]
    pub fn cross(self, o: Self) -> Self {
        Self::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    /// Distance to `o`.
    #[inline]
    pub fn distance(self, o: Self) -> f32 {
        (self - o).length()
    }

    /// `(x, y, z, w)`: `w = 1` for a point, `w = 0` for a direction.
    #[inline]
    pub const fn extend(self, w: f32) -> Vec4 {
        Vec4::new(self.x, self.y, self.z, w)
    }
}

impl Vec4 {
    #[allow(missing_docs)]
    pub const ZERO: Self = Self::splat(0.0);

    #[allow(missing_docs)]
    #[inline]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    /// The first three components.
    #[inline]
    pub const fn xyz(self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_is_right_handed() {
        assert_eq!(Vec3::X.cross(Vec3::Y), Vec3::Z);
        assert_eq!(Vec3::Y.cross(Vec3::Z), Vec3::X);
        assert_eq!(Vec3::Z.cross(Vec3::X), Vec3::Y);
    }

    #[test]
    fn normalize() {
        let v = Vec3::new(3.0, 0.0, 4.0);
        assert_eq!(v.length(), 5.0);
        assert!((v.normalize().length() - 1.0).abs() < 1e-6);
        assert_eq!(Vec3::ZERO.normalize(), Vec3::ZERO, "zero stays zero");
    }

    #[test]
    fn arithmetic() {
        let a = Vec4::new(1.0, 2.0, 3.0, 4.0);
        assert_eq!(a + a, a * 2.0);
        assert_eq!(2.0 * a - a, a);
        assert_eq!(a.dot(a), 30.0);
        assert_eq!(a.xyz().extend(4.0), a);
        assert_eq!(Vec2::new(1.0, 0.0).perp_dot(Vec2::new(0.0, 1.0)), 1.0);
        assert_eq!(Vec3::ZERO.lerp(Vec3::ONE, 0.25), Vec3::splat(0.25));
    }
}
