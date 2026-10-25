# oxstack-gfx

A `no_std` RGB565 framebuffer for SPI displays. Pixels are stored already in
the panel's big-endian wire format, and the buffer tracks which rows changed,
so a flush is one contiguous slice that DMA can send with no conversion or
copy. It implements `embedded-graphics`' `DrawTarget`. Renderers that write
pixels one at a time (such as `oxstack-3d`) use `wire_rows_mut`, which marks
the rows it hands out dirty.

```rust
use embedded_graphics::prelude::*;
use oxstack_gfx::{Framebuffer, Rgb565};

// 64.8 KB: keep it in a `static`. `new()` is all-zero, so it costs no flash.
static mut FB: Framebuffer<240, 135> = Framebuffer::new();
# let fb = unsafe { &mut *core::ptr::addr_of_mut!(FB) };

fb.clear(Rgb565::BLUE).unwrap();
if let Some(dirty) = fb.take_dirty() {
    // send dirty.bytes to rows dirty.y .. dirty.y + dirty.rows
    assert_eq!(dirty.bytes.len(), 240 * 135 * 2);
}
```

Part of [oxstack](https://github.com/theashraf/oxstack). Licensed under the MIT license.
