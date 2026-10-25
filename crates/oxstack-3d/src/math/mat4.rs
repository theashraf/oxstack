use core::ops::{Mul, MulAssign};

use super::{Quat, Vec3, Vec4, sin_cos, tan};

/// A 4×4 matrix of `f32`, column-major, for column vectors: `m * v`
/// transforms `v`, and `a * b` applies `b` first.
///
/// Conventions match OpenGL/glam's right-handed world (+Y up, a camera
/// looks along −Z), with clip-space depth in `0..=1` (near → 0, far → 1)
/// as in Vulkan and Direct3D. That depth range gives the rasteriser one
/// fewer remap than OpenGL's `-1..=1`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
pub struct Mat4 {
    /// Columns: x axis, y axis, z axis, translation.
    pub cols: [Vec4; 4],
}

impl Default for Mat4 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mat4 {
    #[allow(missing_docs)]
    pub const IDENTITY: Self = Self::from_cols(
        Vec4::new(1.0, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 1.0, 0.0, 0.0),
        Vec4::new(0.0, 0.0, 1.0, 0.0),
        Vec4::new(0.0, 0.0, 0.0, 1.0),
    );

    /// From four columns.
    #[inline]
    pub const fn from_cols(x: Vec4, y: Vec4, z: Vec4, w: Vec4) -> Self {
        Self { cols: [x, y, z, w] }
    }

    /// Row `i` (0..4).
    #[inline]
    pub fn row(&self, i: usize) -> Vec4 {
        let c = |v: Vec4| [v.x, v.y, v.z, v.w][i];
        Vec4::new(
            c(self.cols[0]),
            c(self.cols[1]),
            c(self.cols[2]),
            c(self.cols[3]),
        )
    }

    /// Moves points by `t`.
    #[inline]
    pub const fn from_translation(t: Vec3) -> Self {
        let mut m = Self::IDENTITY;
        m.cols[3] = t.extend(1.0);
        m
    }

    /// Scales each axis.
    #[inline]
    pub const fn from_scale(s: Vec3) -> Self {
        Self::from_cols(
            Vec4::new(s.x, 0.0, 0.0, 0.0),
            Vec4::new(0.0, s.y, 0.0, 0.0),
            Vec4::new(0.0, 0.0, s.z, 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        )
    }

    /// Rotation about +X by `angle` radians (counter-clockwise looking
    /// down the axis toward the origin).
    pub fn from_rotation_x(angle: f32) -> Self {
        let (s, c) = sin_cos(angle);
        Self::from_cols(
            Vec4::new(1.0, 0.0, 0.0, 0.0),
            Vec4::new(0.0, c, s, 0.0),
            Vec4::new(0.0, -s, c, 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        )
    }

    /// Rotation about +Y by `angle` radians.
    pub fn from_rotation_y(angle: f32) -> Self {
        let (s, c) = sin_cos(angle);
        Self::from_cols(
            Vec4::new(c, 0.0, -s, 0.0),
            Vec4::new(0.0, 1.0, 0.0, 0.0),
            Vec4::new(s, 0.0, c, 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        )
    }

    /// Rotation about +Z by `angle` radians.
    pub fn from_rotation_z(angle: f32) -> Self {
        let (s, c) = sin_cos(angle);
        Self::from_cols(
            Vec4::new(c, s, 0.0, 0.0),
            Vec4::new(-s, c, 0.0, 0.0),
            Vec4::new(0.0, 0.0, 1.0, 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        )
    }

    /// The rotation `q` (assumed unit length).
    pub fn from_quat(q: Quat) -> Self {
        let (x2, y2, z2) = (q.x + q.x, q.y + q.y, q.z + q.z);
        let (xx, yy, zz) = (q.x * x2, q.y * y2, q.z * z2);
        let (xy, xz, yz) = (q.x * y2, q.x * z2, q.y * z2);
        let (wx, wy, wz) = (q.w * x2, q.w * y2, q.w * z2);
        Self::from_cols(
            Vec4::new(1.0 - (yy + zz), xy + wz, xz - wy, 0.0),
            Vec4::new(xy - wz, 1.0 - (xx + zz), yz + wx, 0.0),
            Vec4::new(xz + wy, yz - wx, 1.0 - (xx + yy), 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        )
    }

    /// Scale, then rotate, then translate: the usual model matrix, built
    /// directly rather than by multiplying three matrices.
    pub fn from_scale_rotation_translation(scale: Vec3, rotation: Quat, translation: Vec3) -> Self {
        let mut m = Self::from_quat(rotation);
        m.cols[0] *= scale.x;
        m.cols[1] *= scale.y;
        m.cols[2] *= scale.z;
        m.cols[3] = translation.extend(1.0);
        m
    }

    /// Right-handed perspective projection, depth `0..=1`.
    ///
    /// `fov_y` is the vertical field of view in radians, `aspect` is
    /// width / height. Depth precision is spread like `1/z`, so most of it
    /// sits just beyond `near`: keep `near` as large as the scene allows.
    pub fn perspective_rh(fov_y: f32, aspect: f32, near: f32, far: f32) -> Self {
        let f = 1.0 / tan(0.5 * fov_y);
        let r = far / (near - far);
        Self::from_cols(
            Vec4::new(f / aspect, 0.0, 0.0, 0.0),
            Vec4::new(0.0, f, 0.0, 0.0),
            Vec4::new(0.0, 0.0, r, -1.0),
            Vec4::new(0.0, 0.0, r * near, 0.0),
        )
    }

    /// Right-handed orthographic projection of the view-space box
    /// `left..right` × `bottom..top` × `-near..-far`, depth `0..=1`.
    pub fn orthographic_rh(
        left: f32,
        right: f32,
        bottom: f32,
        top: f32,
        near: f32,
        far: f32,
    ) -> Self {
        let (rw, rh, rd) = (
            1.0 / (right - left),
            1.0 / (top - bottom),
            1.0 / (near - far),
        );
        Self::from_cols(
            Vec4::new(2.0 * rw, 0.0, 0.0, 0.0),
            Vec4::new(0.0, 2.0 * rh, 0.0, 0.0),
            Vec4::new(0.0, 0.0, rd, 0.0),
            Vec4::new(-(left + right) * rw, -(top + bottom) * rh, near * rd, 1.0),
        )
    }

    /// Right-handed view matrix for a camera at `eye` looking at `target`.
    /// `up` must not be parallel to the view direction.
    pub fn look_at_rh(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        let f = (target - eye).normalize();
        let s = f.cross(up).normalize();
        let u = s.cross(f);
        Self::from_cols(
            Vec4::new(s.x, u.x, -f.x, 0.0),
            Vec4::new(s.y, u.y, -f.y, 0.0),
            Vec4::new(s.z, u.z, -f.z, 0.0),
            Vec4::new(-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0),
        )
    }

    /// Transpose.
    pub fn transpose(&self) -> Self {
        Self::from_cols(self.row(0), self.row(1), self.row(2), self.row(3))
    }

    /// `m * v`.
    #[inline]
    pub fn mul_vec4(&self, v: Vec4) -> Vec4 {
        let [a, b, c, d] = self.cols;
        a * v.x + b * v.y + c * v.z + d * v.w
    }

    /// Transforms point `p` (`w = 1`), ignoring the bottom row: correct for
    /// affine matrices (model, view), not projections.
    #[inline]
    pub fn transform_point3(&self, p: Vec3) -> Vec3 {
        let [a, b, c, d] = self.cols;
        (a * p.x + b * p.y + c * p.z + d).xyz()
    }

    /// Transforms direction `v` (`w = 0`): no translation.
    #[inline]
    pub fn transform_vector3(&self, v: Vec3) -> Vec3 {
        let [a, b, c, _] = self.cols;
        (a * v.x + b * v.y + c * v.z).xyz()
    }

    /// Inverse of an affine matrix (bottom row `0 0 0 1`): any mix of
    /// rotation, scale, shear and translation. Returns `None` if the
    /// matrix is singular (a zero scale, say).
    pub fn inverse_affine(&self) -> Option<Self> {
        let [a, b, c, t] = self.cols.map(Vec4::xyz);
        // Rows of the inverse 3×3 are the cross products of the columns,
        // divided by the determinant (the adjugate formula).
        let (r0, r1, r2) = (b.cross(c), c.cross(a), a.cross(b));
        let det = a.dot(r0);
        if det == 0.0 || !det.is_finite() {
            return None;
        }
        let inv = 1.0 / det;
        let (r0, r1, r2) = (r0 * inv, r1 * inv, r2 * inv);
        Some(Self::from_cols(
            Vec4::new(r0.x, r1.x, r2.x, 0.0),
            Vec4::new(r0.y, r1.y, r2.y, 0.0),
            Vec4::new(r0.z, r1.z, r2.z, 0.0),
            Vec4::new(-r0.dot(t), -r1.dot(t), -r2.dot(t), 1.0),
        ))
    }

    /// The largest scale factor along any of the three axes: how much a
    /// bounding sphere grows under this matrix.
    pub fn max_axis_scale(&self) -> f32 {
        let [a, b, c, _] = self.cols.map(Vec4::xyz);
        super::sqrt(
            a.length_squared()
                .max(b.length_squared())
                .max(c.length_squared()),
        )
    }
}

impl Mul for Mat4 {
    type Output = Self;
    #[inline]
    fn mul(self, o: Self) -> Self {
        Self {
            cols: o.cols.map(|c| self.mul_vec4(c)),
        }
    }
}

impl MulAssign for Mat4 {
    #[inline]
    fn mul_assign(&mut self, o: Self) {
        *self = *self * o;
    }
}

impl Mul<Vec4> for Mat4 {
    type Output = Vec4;
    #[inline]
    fn mul(self, v: Vec4) -> Vec4 {
        self.mul_vec4(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::assert_close;

    fn assert_mat_close(a: &Mat4, b: &Mat4) {
        for (ca, cb) in a.cols.iter().zip(&b.cols) {
            assert_close(*ca, *cb);
        }
    }

    #[test]
    fn perspective_maps_near_to_0_and_far_to_1() {
        let p = Mat4::perspective_rh(1.0, 16.0 / 9.0, 0.5, 40.0);
        let ndc = |z: f32| {
            let c = p * Vec4::new(0.0, 0.0, z, 1.0);
            c.z / c.w
        };
        assert!(ndc(-0.5).abs() < 1e-6, "near");
        assert!((ndc(-40.0) - 1.0).abs() < 1e-6, "far");
        assert!(ndc(-10.0) > 0.9, "most precision sits near the camera");
        // w carries the view depth for the perspective divide.
        assert_eq!((p * Vec4::new(0.0, 0.0, -7.0, 1.0)).w, 7.0);
        // The top of the field of view lands at NDC y = 1.
        let half = 0.5f32;
        let c = p * Vec4::new(0.0, tan(half) * 3.0, -3.0, 1.0);
        assert!((c.y / c.w - 1.0).abs() < 1e-6);
    }

    #[test]
    fn orthographic_maps_box_to_ndc() {
        let p = Mat4::orthographic_rh(-2.0, 6.0, -1.0, 3.0, 1.0, 11.0);
        assert_close(
            p * Vec4::new(-2.0, -1.0, -1.0, 1.0),
            Vec4::new(-1.0, -1.0, 0.0, 1.0),
        );
        assert_close(
            p * Vec4::new(6.0, 3.0, -11.0, 1.0),
            Vec4::new(1.0, 1.0, 1.0, 1.0),
        );
    }

    #[test]
    fn look_at_puts_target_on_negative_z() {
        let eye = Vec3::new(3.0, 4.0, 5.0);
        let target = Vec3::new(-1.0, 0.5, 2.0);
        let v = Mat4::look_at_rh(eye, target, Vec3::Y);
        let t = v.transform_point3(target);
        assert_close(t, Vec3::new(0.0, 0.0, -eye.distance(target)));
        assert_close(v.transform_point3(eye), Vec3::ZERO);
        // Up stays up: a point above the target has positive view y.
        assert!(v.transform_point3(target + Vec3::Y).y > 0.0);
    }

    #[test]
    fn rotations_follow_right_hand_rule() {
        assert_close(
            Mat4::from_rotation_z(core::f32::consts::FRAC_PI_2).transform_vector3(Vec3::X),
            Vec3::Y,
        );
        assert_close(
            Mat4::from_rotation_x(core::f32::consts::FRAC_PI_2).transform_vector3(Vec3::Y),
            Vec3::Z,
        );
        assert_close(
            Mat4::from_rotation_y(core::f32::consts::FRAC_PI_2).transform_vector3(Vec3::Z),
            Vec3::X,
        );
    }

    #[test]
    fn affine_inverse() {
        let m = Mat4::from_scale_rotation_translation(
            Vec3::new(2.0, 0.5, 3.0),
            Quat::from_axis_angle(Vec3::new(1.0, 2.0, 3.0).normalize(), 0.7),
            Vec3::new(4.0, -5.0, 6.0),
        );
        let inv = m.inverse_affine().unwrap();
        assert_mat_close(&(m * inv), &Mat4::IDENTITY);
        assert_mat_close(&(inv * m), &Mat4::IDENTITY);
        assert!(
            Mat4::from_scale(Vec3::new(1.0, 0.0, 1.0))
                .inverse_affine()
                .is_none()
        );
    }

    #[test]
    fn product_order_and_rows() {
        let t = Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0));
        let s = Mat4::from_scale(Vec3::splat(2.0));
        // `t * s` scales first.
        assert_close((t * s).transform_point3(Vec3::X), Vec3::new(3.0, 0.0, 0.0));
        assert_eq!(t.row(0), Vec4::new(1.0, 0.0, 0.0, 1.0));
        assert_eq!(t.transpose().cols[0], t.row(0));
        assert_eq!(s.max_axis_scale(), 2.0);
    }
}
