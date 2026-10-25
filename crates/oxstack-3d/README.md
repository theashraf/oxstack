# oxstack-3d

Software 3D for small displays, `no_std` and allocation-free:

- **Math**: `Vec2`/`Vec3`/`Vec4`, column-major `Mat4`, `Quat`, all `f32`.
  Includes perspective/orthographic projection, `look_at`, affine inverse,
  slerp, and gyro-style `from_scaled_axis`.
- **Camera** with position plus quaternion, `look_at`, local movement, and
  perspective or orthographic projection.
- **Pipeline**: whole-mesh frustum culling with bounding spheres, back-face
  culling in clip space, homogeneous near/far clipping with a guard band,
  flat Lambert shading, distance fog, and depth-tested 3D lines.
- **Rasteriser**: fixed-point edge functions with exact per-row spans and
  a tie-break rule, so shared edges have no gaps and no double writes. It
  uses a 16-bit z-buffer, and the 2D parts of `embedded-graphics` draw into
  the same frame.

```rust
use oxstack_3d::{Camera, DepthBuffer, Renderer, mesh};
use oxstack_3d::math::{Mat4, Vec3, radians};
use oxstack_gfx::{Framebuffer, Rgb565};
use embedded_graphics::prelude::*;

// 64.8 KB each at 240×135: keep them in statics. Both are all-zero,
// so they cost RAM but no flash.
static mut FB: Framebuffer<240, 135> = Framebuffer::new();
static mut DEPTH: DepthBuffer<240, 135> = DepthBuffer::new();
# let (fb, depth) = unsafe { (&mut *core::ptr::addr_of_mut!(FB), &mut *core::ptr::addr_of_mut!(DEPTH)) };

let mut camera = Camera::perspective(radians(60.0), 0.5, 30.0);
camera.position = Vec3::new(3.0, 2.0, 4.0);
camera.look_at(Vec3::ZERO, Vec3::Y);

let mut r = Renderer::new(fb, depth, &camera);
r.clear(Rgb565::BLACK);
r.draw_mesh(&mesh::CUBE, &Mat4::IDENTITY);
assert_eq!(r.stats().rasterized, 6);
```

On a Cardputer ADV (ESP32-S3 at 240 MHz, 240×135), the `scene3d` example
draws 168 faces with fog and lines at **46–49 fps**. Each frame spends about
7–8 ms rendering and 13.2 ms on the DMA flush to the panel.

Part of [oxstack](https://github.com/theashraf/oxstack). Licensed under the MIT license.
