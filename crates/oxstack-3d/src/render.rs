use embedded_graphics_core::geometry::Point;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use oxstack_gfx::{Framebuffer, wire_color};

use crate::clip::{self, MAX_VERTS};
use crate::math::{Mat4, Vec3, Vec4, recip, rsqrt, sqrt};
use crate::mesh::Mesh;
use crate::raster::{self, MAX_DIM, ScreenVertex};
use crate::{Camera, DepthBuffer, Fog, Lighting, shading};

/// Which faces to skip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Cull {
    /// Skip faces seen from behind (clockwise on screen). Right for closed
    /// solids, and halves the work.
    #[default]
    Back,
    /// Skip faces seen from the front: draws the inside of a room or a
    /// sky box.
    Front,
    /// Draw both sides; the back side is lit as if its normal were flipped.
    None,
}

/// What the renderer did since it was created. Cheap to keep; handy for
/// tuning a scene on the device.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Stats {
    /// Meshes skipped whole because their bounding sphere was off-screen.
    pub meshes_culled: u32,
    /// Faces considered.
    pub faces: u32,
    /// Faces skipped by [`Cull`].
    pub backfaces: u32,
    /// Faces entirely outside the view.
    pub outside: u32,
    /// Faces cut by the near/far planes or the guard band.
    pub clipped: u32,
    /// Triangles that reached the rasteriser and covered pixel centres.
    pub rasterized: u32,
}

/// Draws 3D geometry into a framebuffer, depth-tested against a
/// [`DepthBuffer`].
///
/// It borrows both buffers, so make one per frame: setting up is a few
/// matrix multiplies. Drop it (or let it go out of scope) before flushing
/// the framebuffer.
///
/// ```
/// use oxstack_3d::{Camera, DepthBuffer, Renderer, math::{Mat4, Vec3, radians}, mesh};
/// use oxstack_gfx::{Framebuffer, Rgb565};
/// use embedded_graphics::prelude::*;
///
/// static mut FB: Framebuffer<240, 135> = Framebuffer::new();
/// static mut DEPTH: DepthBuffer<240, 135> = DepthBuffer::new();
/// # let (fb, depth) = unsafe { (&mut *core::ptr::addr_of_mut!(FB), &mut *core::ptr::addr_of_mut!(DEPTH)) };
///
/// let mut camera = Camera::perspective(radians(60.0), 0.5, 20.0);
/// camera.position = Vec3::new(0.0, 2.0, 5.0);
/// camera.look_at(Vec3::ZERO, Vec3::Y);
///
/// let mut r = Renderer::new(fb, depth, &camera);
/// r.clear(Rgb565::BLACK);
/// r.draw_mesh(&mesh::CUBE, &Mat4::from_rotation_y(0.5));
/// assert_eq!(r.stats().rasterized, 6, "three sides of the cube face the camera");
/// ```
pub struct Renderer<'a, const W: usize, const H: usize> {
    fb: &'a mut Framebuffer<W, H>,
    depth: &'a mut DepthBuffer<W, H>,
    view_proj: Mat4,
    eye: Vec3,
    /// World-space frustum planes, unit normals pointing inward.
    frustum: [Vec4; 6],
    /// Lighting for faces drawn from now on.
    pub lighting: Lighting,
    /// Face culling for meshes drawn from now on.
    pub cull: Cull,
    /// Distance fog for faces drawn from now on.
    pub fog: Option<Fog>,
    stats: Stats,
}

impl<'a, const W: usize, const H: usize> Renderer<'a, W, H> {
    const ASSERT_SIZE: () = assert!(
        W <= MAX_DIM && H <= MAX_DIM,
        "oxstack-3d supports screens up to 512×512"
    );

    /// A renderer drawing into `fb` and `depth` from `camera`, with
    /// [`Lighting::DEFAULT`], back-face culling and no fog.
    pub fn new(
        fb: &'a mut Framebuffer<W, H>,
        depth: &'a mut DepthBuffer<W, H>,
        camera: &Camera,
    ) -> Self {
        let () = Self::ASSERT_SIZE;
        let mut r = Self {
            fb,
            depth,
            view_proj: Mat4::IDENTITY,
            eye: Vec3::ZERO,
            frustum: [Vec4::ZERO; 6],
            lighting: Lighting::DEFAULT,
            cull: Cull::Back,
            fog: None,
            stats: Stats::default(),
        };
        r.set_camera(camera);
        r
    }

    /// Switches camera mid-frame, e.g. for a picture-in-picture view.
    pub fn set_camera(&mut self, camera: &Camera) {
        self.view_proj = camera.view_projection(W as f32 / H as f32);
        self.eye = camera.position;
        // Gribb–Hartmann: frustum planes are sums of the matrix rows.
        let r = [0, 1, 2, 3].map(|i| self.view_proj.row(i));
        let planes = [
            r[3] + r[0],
            r[3] - r[0],
            r[3] + r[1],
            r[3] - r[1],
            r[2],
            r[3] - r[2],
        ];
        self.frustum = planes.map(|p| p * (1.0 / p.xyz().length()));
    }

    /// Fills the framebuffer with `color` and empties the depth buffer.
    pub fn clear(&mut self, color: Rgb565) {
        let _ = embedded_graphics_core::draw_target::DrawTarget::clear(self.fb, color);
        self.depth.clear();
    }

    /// Empties only the depth buffer: for a background you drew yourself,
    /// or to draw an overlay (a HUD model) on top of everything.
    pub fn clear_depth(&mut self) {
        self.depth.clear();
    }

    /// The framebuffer, for 2D drawing (text, a HUD) between 3D draws.
    pub fn framebuffer(&mut self) -> &mut Framebuffer<W, H> {
        self.fb
    }

    /// Counters since this renderer was created.
    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// Where the world-space point `p` lands on screen, or `None` if it is
    /// outside the view volume. For labels and 2D markers over 3D things.
    pub fn project(&self, p: Vec3) -> Option<Point> {
        let c = self.view_proj * p.extend(1.0);
        if clip::frustum_code(c) != 0 {
            return None;
        }
        let s = self.to_screen(c);
        Some(Point::new(
            libm::floorf(s.x) as i32,
            libm::floorf(s.y) as i32,
        ))
    }

    /// Draws `mesh` transformed by `model` (model → world), in its faces'
    /// colours.
    pub fn draw_mesh(&mut self, mesh: &Mesh<'_>, model: &Mat4) {
        self.mesh(mesh, model, None);
    }

    /// Draws `mesh` with every face in `color`.
    pub fn draw_mesh_colored(&mut self, mesh: &Mesh<'_>, model: &Mat4, color: Rgb565) {
        self.mesh(mesh, model, Some(color));
    }

    /// Draws a one-pixel world-space line, depth-tested but unlit. Lines
    /// win depth ties, so an edge drawn over a face stays visible.
    pub fn draw_line(&mut self, a: Vec3, b: Vec3, color: Rgb565) {
        let (ca, cb) = (
            self.view_proj * a.extend(1.0),
            self.view_proj * b.extend(1.0),
        );
        if clip::frustum_code(ca) & clip::frustum_code(cb) != 0 {
            return;
        }
        let planes = clip::guard_code(ca) | clip::guard_code(cb);
        let Some((ca, cb)) = clip::segment(ca, cb, planes) else {
            return;
        };
        let (sa, sb) = (self.to_screen(ca), self.to_screen(cb));
        raster::line(self.fb, self.depth, sa, sb, wire_color(color));
    }

    fn to_screen(&self, c: Vec4) -> ScreenVertex {
        let inv_w = recip(c.w);
        ScreenVertex {
            x: (c.x * inv_w + 1.0) * (0.5 * W as f32),
            y: (1.0 - c.y * inv_w) * (0.5 * H as f32),
            near: 1.0 - c.z * inv_w,
        }
    }

    fn mesh(&mut self, mesh: &Mesh<'_>, model: &Mat4, color: Option<Rgb565>) {
        // Whole-mesh rejection: bounding sphere against the frustum.
        let centre = model.cols[3].xyz();
        let radius = sqrt(mesh.bounding_radius_squared()) * model.max_axis_scale();
        if self
            .frustum
            .iter()
            .any(|p| p.xyz().dot(centre) + p.w < -radius)
        {
            self.stats.meshes_culled += 1;
            return;
        }

        let mvp = self.view_proj * *model;
        let sun = self.lighting.direction.normalize();
        let verts = mesh.vertices();
        for face in mesh.faces() {
            self.stats.faces += 1;
            let p = face.indices.map(|i| verts[i as usize]);
            let c = p.map(|p| mvp * p.extend(1.0));

            // Facing, from the clip-space (x, y, w) determinant: its sign
            // is the screen winding, and unlike a screen-space test it is
            // right before clipping, even for vertices behind the camera.
            let xyw = c.map(|c| Vec3::new(c.x, c.y, c.w));
            let det = xyw[0].dot(xyw[1].cross(xyw[2]));
            let front = det > 0.0;
            let skip = match self.cull {
                Cull::Back => !front,
                Cull::Front => front,
                Cull::None => det == 0.0,
            };
            if skip {
                self.stats.backfaces += 1;
                continue;
            }

            let codes = c.map(clip::frustum_code);
            if codes[0] & codes[1] & codes[2] != 0 {
                self.stats.outside += 1;
                continue;
            }

            // Flat shading from the world-space normal. The cross product
            // of transformed edges is correct even under non-uniform scale.
            let w = p.map(|p| model.transform_point3(p));
            let mut n = (w[1] - w[0]).cross(w[2] - w[0]);
            if !front {
                n = -n;
            }
            let facing = n.dot(sun);
            let diffuse = if facing > 0.0 {
                facing * rsqrt(n.length_squared())
            } else {
                0.0
            };
            let light = self.lighting.ambient + self.lighting.intensity * diffuse;
            let (fog_color, fog) = match self.fog {
                Some(f) => {
                    let d2 = ((w[0] + w[1] + w[2]) * (1.0 / 3.0) - self.eye).length_squared();
                    (
                        f.color,
                        f.amount(if d2 > 0.0 { d2 * rsqrt(d2) } else { 0.0 }),
                    )
                }
                None => (Rgb565::BLACK, 0.0),
            };
            let base = color.unwrap_or(face.color);
            let wire = wire_color(shading::shade(base, light, fog_color, fog));

            let planes = c.map(clip::guard_code);
            let planes = planes[0] | planes[1] | planes[2];
            if planes == 0 {
                let s = c.map(|c| self.to_screen(c));
                self.stats.rasterized += raster::fill_triangle(self.fb, self.depth, s, wire) as u32;
                continue;
            }

            self.stats.clipped += 1;
            let mut poly = [Vec4::ZERO; MAX_VERTS];
            poly[..3].copy_from_slice(&c);
            let n = clip::polygon(&mut poly, 3, planes);
            if n < 3 {
                continue;
            }
            let first = self.to_screen(poly[0]);
            let mut prev = self.to_screen(poly[1]);
            for v in &poly[2..n] {
                let next = self.to_screen(*v);
                self.stats.rasterized +=
                    raster::fill_triangle(self.fb, self.depth, [first, prev, next], wire) as u32;
                prev = next;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{Quat, radians};
    use crate::mesh::{CUBE, Face, Mesh};

    const W: usize = 96;
    const H: usize = 64;
    const BG: Rgb565 = Rgb565::new(1, 2, 3);

    struct Target {
        fb: Framebuffer<W, H>,
        depth: DepthBuffer<W, H>,
    }

    fn target() -> std::boxed::Box<Target> {
        std::boxed::Box::new(Target {
            fb: Framebuffer::new(),
            depth: DepthBuffer::new(),
        })
    }
    extern crate std;

    fn camera_at(eye: Vec3, target: Vec3) -> Camera {
        let mut c = Camera::perspective(radians(60.0), 0.1, 100.0);
        c.position = eye;
        c.look_at(target, Vec3::Y);
        c
    }

    fn background_pixels(fb: &Framebuffer<W, H>, rows: core::ops::Range<usize>) -> usize {
        rows.flat_map(|y| (0..W).map(move |x| (x, y)))
            .filter(|&(x, y)| fb.pixel(x, y) == Some(BG))
            .count()
    }

    #[test]
    fn cube_from_outside_shows_three_sides() {
        for cam in [camera_at(Vec3::new(3.0, 4.0, 5.0), Vec3::ZERO), {
            let mut c = Camera::orthographic(6.0, 0.1, 50.0);
            c.position = Vec3::new(-5.0, 3.0, 4.0);
            c.look_at(Vec3::ZERO, Vec3::Y);
            c
        }] {
            let mut t = target();
            let mut r = Renderer::new(&mut t.fb, &mut t.depth, &cam);
            r.clear(BG);
            r.draw_mesh(&CUBE, &Mat4::IDENTITY);
            let s = r.stats();
            assert_eq!((s.faces, s.backfaces, s.rasterized), (12, 6, 6), "{cam:?}");
            // Nothing drawn twice means the near sides hid the far ones,
            // and the centre pixel is cube, not background.
            assert_ne!(t.fb.pixel(W / 2, H / 2), Some(BG));
        }
    }

    #[test]
    fn inside_a_cube() {
        let cam = camera_at(Vec3::ZERO, Vec3::new(0.3, 0.2, -1.0));
        let room = Mat4::from_scale(Vec3::splat(5.0));

        let mut t = target();
        let mut r = Renderer::new(&mut t.fb, &mut t.depth, &cam);
        r.clear(BG);
        r.draw_mesh(&CUBE, &room);
        assert_eq!(
            (r.stats().backfaces, r.stats().rasterized),
            (12, 0),
            "all walls face away"
        );

        r.cull = Cull::Front;
        r.draw_mesh(&CUBE, &room);
        assert_eq!(
            background_pixels(&t.fb, 0..H),
            0,
            "walls cover the whole screen"
        );
    }

    /// A 10×10 grid of quads, 2 units each, on y = 0.
    fn floor() -> (std::vec::Vec<Vec3>, std::vec::Vec<Face>) {
        let n = 10;
        let mut v = std::vec::Vec::new();
        for z in 0..=n {
            for x in 0..=n {
                v.push(Vec3::new(
                    (x as f32 - 5.0) * 2.0,
                    0.0,
                    (z as f32 - 5.0) * 2.0,
                ));
            }
        }
        let mut f = std::vec::Vec::new();
        let idx = |x: u16, z: u16| z * (n + 1) + x;
        for z in 0..n {
            for x in 0..n {
                // Counter-clockwise seen from above (+Y).
                let c = if (x + z) % 2 == 0 {
                    Rgb565::WHITE
                } else {
                    Rgb565::RED
                };
                f.push(Face::new(idx(x, z), idx(x, z + 1), idx(x + 1, z + 1), c));
                f.push(Face::new(idx(x, z), idx(x + 1, z + 1), idx(x + 1, z), c));
            }
        }
        (v, f)
    }

    #[test]
    fn floor_through_the_near_plane_has_no_cracks() {
        let (v, f) = floor();
        let floor = Mesh::new(&v, &f);
        // Low over the floor, looking slightly down and sideways: triangles
        // under the camera cross the near plane, ones at the sides cross
        // the guard band.
        for (i, yaw) in [0.0f32, 0.4, 1.3, 2.9].into_iter().enumerate() {
            let mut cam = Camera::perspective(radians(70.0), 0.05, 100.0);
            cam.position = Vec3::new(0.3 * i as f32, 0.4, 0.7);
            cam.rotation = Quat::from_yaw_pitch_roll(yaw, radians(-20.0), 0.1);
            let mut t = target();
            let mut r = Renderer::new(&mut t.fb, &mut t.depth, &cam);
            r.clear(BG);
            r.draw_mesh(&floor, &Mat4::IDENTITY);
            assert!(r.stats().clipped > 0, "the test must exercise clipping");
            // The floor is 10 units across and we look down at 20°: the
            // bottom 40% of the screen is floor everywhere.
            assert_eq!(
                background_pixels(&t.fb, H * 6 / 10..H),
                0,
                "crack at yaw {yaw}"
            );
        }
    }

    #[test]
    fn far_plane_and_sphere_culling() {
        let mut cam = Camera::perspective(radians(60.0), 0.5, 20.0);
        cam.position = Vec3::new(0.0, 1.0, 0.0);
        let mut t = target();
        let mut r = Renderer::new(&mut t.fb, &mut t.depth, &cam);
        r.clear(BG);
        r.draw_mesh(&CUBE, &Mat4::from_translation(Vec3::new(0.0, 0.0, -30.0)));
        r.draw_mesh(&CUBE, &Mat4::from_translation(Vec3::new(0.0, 0.0, 5.0)));
        assert_eq!(
            (r.stats().meshes_culled, r.stats().faces),
            (2, 0),
            "beyond far, behind"
        );

        // A long box below the camera, from z = 3 to z = −27, crosses the
        // far plane: it is cut there, not dropped.
        r.draw_mesh(
            &CUBE,
            &Mat4::from_scale_rotation_translation(
                Vec3::new(1.0, 1.0, 15.0),
                Quat::IDENTITY,
                Vec3::new(0.0, -2.0, -12.0),
            ),
        );
        assert!(r.stats().clipped > 0 && r.stats().rasterized > 0);
        assert!(t.depth.get(W / 2, H - 1).unwrap() > 0, "near end drawn");
        // Row 36's ray meets the box top 24.6 away (beyond far, before the
        // box ends at 27); row 40's meets it 13 away.
        assert_eq!(t.fb.pixel(W / 2, 36), Some(BG), "cut at the far plane");
        assert_ne!(t.fb.pixel(W / 2, 40), Some(BG));
    }

    #[test]
    fn projection_and_lines() {
        let cam = camera_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO);
        let mut t = target();
        let mut r = Renderer::new(&mut t.fb, &mut t.depth, &cam);
        r.clear(BG);
        assert_eq!(
            r.project(Vec3::ZERO),
            Some(Point::new(W as i32 / 2, H as i32 / 2))
        );
        assert_eq!(
            r.project(Vec3::new(0.0, 0.0, 9.0)),
            None,
            "behind the camera"
        );
        // A line through the camera is clipped at the near plane.
        r.draw_line(
            Vec3::new(0.0, -1.0, 10.0),
            Vec3::new(0.0, -1.0, -10.0),
            Rgb565::GREEN,
        );
        assert_eq!(t.fb.pixel(W / 2, H - 1), Some(Rgb565::GREEN));
    }

    #[test]
    fn lighting_follows_the_sun() {
        let cam = camera_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO);
        let mut t = target();
        let mut r = Renderer::new(&mut t.fb, &mut t.depth, &cam);
        r.lighting = Lighting {
            ambient: 0.2,
            direction: Vec3::Z,
            intensity: 0.8,
        };
        r.clear(BG);
        r.draw_mesh_colored(&CUBE, &Mat4::IDENTITY, Rgb565::new(20, 40, 20));
        assert_eq!(
            t.fb.pixel(W / 2, H / 2),
            Some(Rgb565::new(20, 40, 20)),
            "fully lit"
        );

        let mut r = Renderer::new(&mut t.fb, &mut t.depth, &cam);
        r.lighting.direction = -Vec3::Z;
        r.lighting.ambient = 0.5;
        r.clear(BG);
        r.draw_mesh_colored(&CUBE, &Mat4::IDENTITY, Rgb565::new(20, 40, 20));
        assert_eq!(
            t.fb.pixel(W / 2, H / 2),
            Some(Rgb565::new(10, 20, 10)),
            "ambient only"
        );
    }
}
