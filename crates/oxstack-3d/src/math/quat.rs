use core::ops::{Mul, MulAssign};

use super::{Vec3, acos, rsqrt, sin_cos, sqrt};

/// A rotation, stored as a unit quaternion `w + xi + yj + zk`.
///
/// Prefer it to Euler angles for anything that turns continuously, such as
/// a spinning model, a flight camera, or gyro integration: composing two
/// quaternions is 16 multiplies, it never gimbal-locks, and drift is
/// fixed by [`normalize`](Self::normalize).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
#[allow(missing_docs)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Default for Quat {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Quat {
    /// No rotation.
    pub const IDENTITY: Self = Self::from_xyzw(0.0, 0.0, 0.0, 1.0);

    /// From raw components; the caller keeps it unit length.
    #[inline]
    pub const fn from_xyzw(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    /// Rotation by `angle` radians about the unit vector `axis`,
    /// counter-clockwise when the axis points at the viewer.
    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Self {
        let (s, c) = sin_cos(0.5 * angle);
        let v = axis * s;
        Self::from_xyzw(v.x, v.y, v.z, c)
    }

    /// Rotation by `|v|` radians about `v`: an angular velocity times a
    /// time step, such as a gyro reading in rad/s times `dt`.
    pub fn from_scaled_axis(v: Vec3) -> Self {
        let angle = v.length();
        if angle == 0.0 {
            Self::IDENTITY
        } else {
            Self::from_axis_angle(v * (1.0 / angle), angle)
        }
    }

    /// Rotation about +X.
    pub fn from_rotation_x(angle: f32) -> Self {
        Self::from_axis_angle(Vec3::X, angle)
    }

    /// Rotation about +Y.
    pub fn from_rotation_y(angle: f32) -> Self {
        Self::from_axis_angle(Vec3::Y, angle)
    }

    /// Rotation about +Z.
    pub fn from_rotation_z(angle: f32) -> Self {
        Self::from_axis_angle(Vec3::Z, angle)
    }

    /// Yaw about +Y, then pitch about the turned +X, then roll about the
    /// turned −Z: the usual aircraft or first-person camera angles, since
    /// a camera looks along −Z. Positive pitch looks up; positive roll
    /// banks right.
    pub fn from_yaw_pitch_roll(yaw: f32, pitch: f32, roll: f32) -> Self {
        Self::from_rotation_y(yaw) * Self::from_rotation_x(pitch) * Self::from_rotation_z(-roll)
    }

    /// The rotation whose matrix has these orthonormal columns
    /// (right-handed: `z = x × y`).
    pub fn from_axes(x: Vec3, y: Vec3, z: Vec3) -> Self {
        // Shepperd's method: divide by the largest of the four candidates,
        // so the square root never sees a value near zero.
        let trace = x.x + y.y + z.z;
        let q = if trace > 0.0 {
            let s = 0.5 / sqrt(trace + 1.0);
            Self::from_xyzw((y.z - z.y) * s, (z.x - x.z) * s, (x.y - y.x) * s, 0.25 / s)
        } else if x.x > y.y && x.x > z.z {
            let s = 0.5 / sqrt(1.0 + x.x - y.y - z.z);
            Self::from_xyzw(0.25 / s, (y.x + x.y) * s, (z.x + x.z) * s, (y.z - z.y) * s)
        } else if y.y > z.z {
            let s = 0.5 / sqrt(1.0 + y.y - x.x - z.z);
            Self::from_xyzw((y.x + x.y) * s, 0.25 / s, (z.y + y.z) * s, (z.x - x.z) * s)
        } else {
            let s = 0.5 / sqrt(1.0 + z.z - x.x - y.y);
            Self::from_xyzw((z.x + x.z) * s, (z.y + y.z) * s, 0.25 / s, (x.y - y.x) * s)
        };
        q.normalize()
    }

    /// The rotation that turns −Z to `forward` and keeps +Y as close to
    /// `up` as possible: how a camera at the origin looks along `forward`.
    /// If `forward` is (nearly) parallel to `up`, another up is chosen.
    pub fn looking_to(forward: Vec3, up: Vec3) -> Self {
        let f = forward.normalize();
        let mut right = f.cross(up);
        if right.length_squared() < 1e-12 {
            // Looking straight along `up`: any perpendicular will do.
            let other = if f.x.abs() < 0.9 { Vec3::X } else { Vec3::Z };
            right = other.cross(f);
        }
        let right = right.normalize();
        Self::from_axes(right, right.cross(f), -f)
    }

    /// Squared length; 1 for a rotation.
    #[inline]
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    /// Four-component dot product.
    #[inline]
    pub fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z + self.w * o.w
    }

    /// Rescaled to unit length. Call now and then on a quaternion that is
    /// updated every frame, to stop rounding error building up.
    pub fn normalize(self) -> Self {
        let inv = rsqrt(self.length_squared());
        Self::from_xyzw(self.x * inv, self.y * inv, self.z * inv, self.w * inv)
    }

    /// The opposite rotation (for a unit quaternion).
    #[inline]
    pub fn inverse(self) -> Self {
        Self::from_xyzw(-self.x, -self.y, -self.z, self.w)
    }

    /// Rotates `v`.
    #[inline]
    pub fn rotate(self, v: Vec3) -> Vec3 {
        // v' = v + w·t + q×t with t = 2(q×v): 15 multiplies, not 27.
        let q = Vec3::new(self.x, self.y, self.z);
        let t = q.cross(v) * 2.0;
        v + t * self.w + q.cross(t)
    }

    /// Spherical interpolation along the shortest arc: constant angular
    /// speed in `t`.
    pub fn slerp(self, to: Self, t: f32) -> Self {
        let mut cos = self.dot(to);
        // q and −q are the same rotation; take the short way round.
        let to = if cos < 0.0 {
            cos = -cos;
            Self::from_xyzw(-to.x, -to.y, -to.z, -to.w)
        } else {
            to
        };
        if cos > 0.9995 {
            // Nearly equal: lerp avoids dividing by sin(θ) ≈ 0.
            return self.lerp_raw(to, t).normalize();
        }
        let theta = acos(cos);
        let (sin_theta, _) = sin_cos(theta);
        let (a, _) = sin_cos((1.0 - t) * theta);
        let (b, _) = sin_cos(t * theta);
        let inv = 1.0 / sin_theta;
        self.scale(a * inv).add(to.scale(b * inv))
    }

    /// Normalised linear interpolation: cheaper than `slerp`, and fine for
    /// small steps.
    pub fn nlerp(self, to: Self, t: f32) -> Self {
        let to = if self.dot(to) < 0.0 {
            to.scale(-1.0)
        } else {
            to
        };
        self.lerp_raw(to, t).normalize()
    }

    fn scale(self, s: f32) -> Self {
        Self::from_xyzw(self.x * s, self.y * s, self.z * s, self.w * s)
    }

    fn add(self, o: Self) -> Self {
        Self::from_xyzw(self.x + o.x, self.y + o.y, self.z + o.z, self.w + o.w)
    }

    fn lerp_raw(self, to: Self, t: f32) -> Self {
        self.scale(1.0 - t).add(to.scale(t))
    }
}

/// `a * b` rotates by `b` first, then by `a`, like matrices.
impl Mul for Quat {
    type Output = Self;
    #[inline]
    fn mul(self, b: Self) -> Self {
        let a = self;
        Self::from_xyzw(
            a.w * b.x + a.x * b.w + a.y * b.z - a.z * b.y,
            a.w * b.y - a.x * b.z + a.y * b.w + a.z * b.x,
            a.w * b.z + a.x * b.y - a.y * b.x + a.z * b.w,
            a.w * b.w - a.x * b.x - a.y * b.y - a.z * b.z,
        )
    }
}

impl MulAssign for Quat {
    #[inline]
    fn mul_assign(&mut self, o: Self) {
        *self = *self * o;
    }
}

impl Mul<Vec3> for Quat {
    type Output = Vec3;
    #[inline]
    fn mul(self, v: Vec3) -> Vec3 {
        self.rotate(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{Mat4, assert_close};
    use core::f32::consts::FRAC_PI_2;

    fn q_close(a: Quat, b: Quat) {
        // q and −q are the same rotation.
        assert!(a.dot(b).abs() > 1.0 - 1e-5, "{a:?} vs {b:?}");
    }

    #[test]
    fn rotate_matches_matrix() {
        let q = Quat::from_axis_angle(Vec3::new(-2.0, 1.0, 0.5).normalize(), 2.1);
        let m = Mat4::from_quat(q);
        for v in [Vec3::X, Vec3::Y, Vec3::Z, Vec3::new(0.3, -4.0, 2.0)] {
            assert_close(q * v, m.transform_vector3(v));
        }
        assert_close(Quat::from_rotation_z(FRAC_PI_2) * Vec3::X, Vec3::Y);
    }

    #[test]
    fn product_applies_right_first() {
        let a = Quat::from_rotation_x(0.4);
        let b = Quat::from_rotation_y(1.1);
        let v = Vec3::new(1.0, 2.0, 3.0);
        assert_close((a * b) * v, a * (b * v));
        assert_close(a.inverse() * (a * v), v);
    }

    #[test]
    fn from_axes_round_trips() {
        for q in [
            Quat::IDENTITY,
            Quat::from_rotation_x(3.0),
            Quat::from_rotation_y(-2.9),
            Quat::from_rotation_z(3.1),
            Quat::from_axis_angle(Vec3::new(1.0, 1.0, -1.0).normalize(), 2.5),
        ] {
            q_close(Quat::from_axes(q * Vec3::X, q * Vec3::Y, q * Vec3::Z), q);
        }
    }

    #[test]
    fn looking_to() {
        let f = Vec3::new(1.0, -0.5, -2.0).normalize();
        let q = Quat::looking_to(f, Vec3::Y);
        assert_close(q * -Vec3::Z, f);
        assert!((q * Vec3::Y).y > 0.0, "up stays up");
        assert!((q * Vec3::X).y.abs() < 1e-6, "no roll");
        // Straight down: still a valid rotation.
        let down = Quat::looking_to(-Vec3::Y, Vec3::Y);
        assert_close(down * -Vec3::Z, -Vec3::Y);
    }

    #[test]
    fn yaw_pitch_roll() {
        let fwd = |q: Quat| q * -Vec3::Z;
        assert_close(
            fwd(Quat::from_yaw_pitch_roll(FRAC_PI_2, 0.0, 0.0)),
            -Vec3::X,
        );
        assert_close(fwd(Quat::from_yaw_pitch_roll(0.0, FRAC_PI_2, 0.0)), Vec3::Y);
        // Banking right drops the right wing.
        let right = Quat::from_yaw_pitch_roll(0.0, 0.0, 0.3) * Vec3::X;
        assert!(right.y < 0.0);
    }

    #[test]
    fn interpolation() {
        let a = Quat::from_rotation_y(0.2);
        let b = Quat::from_rotation_y(1.4);
        q_close(a.slerp(b, 0.5), Quat::from_rotation_y(0.8));
        q_close(a.slerp(b, 0.0), a);
        q_close(a.nlerp(b, 1.0), b);
        // Gyro integration: 100 steps of 0.01 rad about Z make 1 rad.
        let mut q = Quat::IDENTITY;
        for _ in 0..100 {
            q *= Quat::from_scaled_axis(Vec3::new(0.0, 0.0, 0.01));
        }
        q_close(q, Quat::from_rotation_z(1.0));
    }
}
