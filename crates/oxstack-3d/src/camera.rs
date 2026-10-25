use crate::math::{Mat4, Quat, Vec3, Vec4};

/// How a [`Camera`] maps view space onto the screen.
///
/// Depth is stored in 16 bits, and with perspective its precision falls
/// off with the square of distance: two surfaces at distance `d` separate
/// cleanly if they are more than about `d² / (near · 65536)` apart. With
/// `near = 0.1` that is 0.015 at `d = 10` but 1.5 at `d = 100`. Pushing
/// `near` out helps far more than pulling `far` in.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Projection {
    /// Things shrink with distance.
    Perspective {
        /// Vertical field of view, radians.
        fov_y: f32,
        /// Distance to the near plane (> 0).
        near: f32,
        /// Distance to the far plane.
        far: f32,
    },
    /// No foreshortening: CAD views, isometric games, maps.
    Orthographic {
        /// World units visible from the bottom to the top of the screen.
        height: f32,
        /// Distance to the near plane (may be negative).
        near: f32,
        /// Distance to the far plane.
        far: f32,
    },
}

impl Projection {
    /// The projection matrix for a screen `aspect` = width / height.
    pub fn matrix(&self, aspect: f32) -> Mat4 {
        match *self {
            Self::Perspective { fov_y, near, far } => {
                Mat4::perspective_rh(fov_y, aspect, near, far)
            }
            Self::Orthographic { height, near, far } => {
                let (h, w) = (0.5 * height, 0.5 * height * aspect);
                Mat4::orthographic_rh(-w, w, -h, h, near, far)
            }
        }
    }
}

/// A camera: where it is, which way it faces, and how it projects.
///
/// It looks along its local −Z with +Y up, like OpenGL. Move it by setting
/// `position`, turn it by setting `rotation` (or [`look_at`](Self::look_at)),
/// and hand it to a [`Renderer`](crate::Renderer).
///
/// ```
/// use oxstack_3d::{Camera, math::{Vec3, radians}};
///
/// let mut camera = Camera::perspective(radians(60.0), 0.1, 50.0);
/// camera.position = Vec3::new(0.0, 2.0, 6.0);
/// camera.look_at(Vec3::ZERO, Vec3::Y);
/// assert!(camera.forward().z < 0.0);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Camera {
    /// Position in world space.
    pub position: Vec3,
    /// Orientation: camera-local axes to world.
    pub rotation: Quat,
    /// Perspective or orthographic.
    pub projection: Projection,
}

impl Camera {
    /// A perspective camera at the origin looking along −Z.
    pub fn perspective(fov_y: f32, near: f32, far: f32) -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            projection: Projection::Perspective { fov_y, near, far },
        }
    }

    /// An orthographic camera at the origin looking along −Z, showing
    /// `height` world units top to bottom.
    pub fn orthographic(height: f32, near: f32, far: f32) -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            projection: Projection::Orthographic { height, near, far },
        }
    }

    /// Turns to face `target`, keeping `up` (usually [`Vec3::Y`]) as close
    /// to screen-up as possible.
    pub fn look_at(&mut self, target: Vec3, up: Vec3) {
        self.rotation = Quat::looking_to(target - self.position, up);
    }

    /// The direction the camera faces.
    pub fn forward(&self) -> Vec3 {
        self.rotation * -Vec3::Z
    }

    /// Screen-right in world space.
    pub fn right(&self) -> Vec3 {
        self.rotation * Vec3::X
    }

    /// Screen-up in world space.
    pub fn up(&self) -> Vec3 {
        self.rotation * Vec3::Y
    }

    /// Moves by `d` in the camera's own axes: `(0, 0, -1)` is one unit
    /// forward, `(1, 0, 0)` one unit right. For flying and walking.
    pub fn translate_local(&mut self, d: Vec3) {
        self.position += self.rotation * d;
    }

    /// World → view space: the inverse of the camera's pose.
    pub fn view(&self) -> Mat4 {
        let mut m = Mat4::from_quat(self.rotation.inverse());
        let t = m.transform_vector3(-self.position);
        m.cols[3] = Vec4::new(t.x, t.y, t.z, 1.0);
        m
    }

    /// World → clip space for a screen `aspect` = width / height.
    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        self.projection.matrix(aspect) * self.view()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{assert_close, radians};

    #[test]
    fn view_matches_look_at_matrix() {
        let mut cam = Camera::perspective(radians(60.0), 0.1, 10.0);
        cam.position = Vec3::new(2.0, 3.0, 4.0);
        let target = Vec3::new(-1.0, 0.0, 0.5);
        cam.look_at(target, Vec3::Y);
        let reference = Mat4::look_at_rh(cam.position, target, Vec3::Y);
        for p in [Vec3::ZERO, target, Vec3::new(5.0, -2.0, 1.0)] {
            assert_close(
                cam.view().transform_point3(p),
                reference.transform_point3(p),
            );
        }
    }

    #[test]
    fn local_movement() {
        let mut cam = Camera::perspective(1.0, 0.1, 10.0);
        cam.rotation = Quat::from_rotation_y(radians(90.0)); // facing −X
        cam.translate_local(Vec3::new(0.0, 0.0, -2.0));
        assert_close(cam.position, Vec3::new(-2.0, 0.0, 0.0));
        assert_close(cam.right(), -Vec3::Z);
        assert_close(cam.up(), Vec3::Y);
    }
}
