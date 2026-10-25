//! Software 3D for small displays: an `f32` math toolbox, a camera with
//! perspective and orthographic projection, homogeneous clipping, back-face
//! and frustum culling, flat shading with fog, and a z-buffered fixed-point
//! rasteriser. `no_std`, no allocation, no `unsafe`.
//!
//! It draws into an [`oxstack_gfx::Framebuffer`], so 2D text and shapes from
//! `embedded-graphics` mix freely with 3D in the same frame.
//!
//! ```
//! use oxstack_3d::{Camera, DepthBuffer, Fog, Renderer, mesh};
//! use oxstack_3d::math::{Mat4, Quat, Vec3, radians};
//! use oxstack_gfx::{Framebuffer, Rgb565};
//! use embedded_graphics::prelude::*;
//!
//! // 64.8 KB each at 240×135. Both are all-zero, so they cost no flash.
//! static mut FB: Framebuffer<240, 135> = Framebuffer::new();
//! static mut DEPTH: DepthBuffer<240, 135> = DepthBuffer::new();
//! # let (fb, depth) = unsafe { (&mut *core::ptr::addr_of_mut!(FB), &mut *core::ptr::addr_of_mut!(DEPTH)) };
//!
//! let mut camera = Camera::perspective(radians(60.0), 0.5, 30.0);
//! camera.position = Vec3::new(0.0, 1.5, 5.0);
//! camera.look_at(Vec3::ZERO, Vec3::Y);
//!
//! let sky = Rgb565::new(4, 8, 12);
//! let mut r = Renderer::new(fb, depth, &camera);
//! r.clear(sky);
//! r.fog = Some(Fog { color: sky, start: 8.0, end: 25.0 });
//! let spin = Quat::from_rotation_y(0.6) * Quat::from_rotation_x(0.3);
//! r.draw_mesh(&mesh::ICOSAHEDRON, &Mat4::from_scale_rotation_translation(Vec3::ONE, spin, Vec3::ZERO));
//! r.draw_line(Vec3::new(-3.0, -1.0, 0.0), Vec3::new(3.0, -1.0, 0.0), Rgb565::WHITE);
//! // `r`'s borrow ends at its last use: now flush `fb` to the display.
//! ```
//!
//! ## Conventions
//!
//! Right-handed world, +Y up, column vectors (`m * v`), and a camera that
//! looks along its local −Z, as in OpenGL and glam. Clip-space depth is
//! `0..=1`. Front faces wind counter-clockwise. Screens up to 512×512
//! are supported.
#![no_std]
#![warn(missing_docs)]
#![deny(unsafe_code)]

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

mod camera;
mod clip;
mod depth;
pub mod math;
pub mod mesh;
mod raster;
mod render;
mod shading;

pub use camera::{Camera, Projection};
pub use depth::DepthBuffer;
pub use render::{Cull, Renderer, Stats};
pub use shading::{Fog, Lighting};
