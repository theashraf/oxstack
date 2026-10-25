use core::convert::Infallible;
use core::ops::Range;

use embedded_graphics_core::Pixel;
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::geometry::{OriginDimensions, Point, Size};
use embedded_graphics_core::pixelcolor::{Rgb565, raw::RawU16};
use embedded_graphics_core::prelude::{Dimensions, IntoStorage};
use embedded_graphics_core::primitives::Rectangle;

/// A `W`×`H` RGB565 framebuffer in display wire format.
///
/// At 240×135 it is 64 800 bytes, so it should live in a `static`, never on
/// the stack. [`Framebuffer::new`] is `const` and all-zero, which places the
/// buffer in `.bss`: it costs RAM but **no flash**. A non-zero initial value
/// would put 64 KB of initialiser data in the firmware image.
///
/// ```
/// use oxstack_gfx::{Framebuffer, Rgb565};
/// use embedded_graphics::prelude::*;
///
/// static mut FB: Framebuffer<240, 135> = Framebuffer::new();
/// # let fb = unsafe { &mut *core::ptr::addr_of_mut!(FB) };
/// fb.clear(Rgb565::BLUE).unwrap();
/// let dirty = fb.take_dirty().unwrap();
/// assert_eq!((dirty.y, dirty.rows), (0, 135));
/// assert_eq!(dirty.bytes.len(), 240 * 135 * 2);
/// ```
pub struct Framebuffer<const W: usize, const H: usize> {
    /// Pixels as big-endian RGB565 (`u16::to_be` of the colour's storage).
    rows: [[u16; W]; H],
    /// Changed rows as the half-open range `dirty_start..dirty_end`; empty
    /// when `dirty_start >= dirty_end`. Zero-initialised means "clean", which
    /// keeps `new()` all-zero.
    dirty_start: u16,
    dirty_end: u16,
}

/// A run of changed, full-width rows ready to be sent to the panel.
#[derive(Debug)]
pub struct DirtyRows<'a> {
    /// First row.
    pub y: u16,
    /// Number of rows.
    pub rows: u16,
    /// `rows * W * 2` bytes of big-endian RGB565.
    pub bytes: &'a [u8],
}

impl<const W: usize, const H: usize> Framebuffer<W, H> {
    const ASSERT_FITS_U16: () =
        assert!(W > 0 && H > 0 && W <= u16::MAX as usize && H <= u16::MAX as usize);

    /// An all-black framebuffer with nothing marked dirty.
    pub const fn new() -> Self {
        let () = Self::ASSERT_FITS_U16;
        Self {
            rows: [[0; W]; H],
            dirty_start: 0,
            dirty_end: 0,
        }
    }

    /// Width in pixels.
    pub const fn width(&self) -> usize {
        W
    }

    /// Height in pixels.
    pub const fn height(&self) -> usize {
        H
    }

    /// Colour at `(x, y)`, or `None` outside the buffer.
    pub fn pixel(&self, x: usize, y: usize) -> Option<Rgb565> {
        let raw = *self.rows.get(y)?.get(x)?;
        Some(RawU16::new(u16::from_be(raw)).into())
    }

    /// Marks every row as changed, e.g. after the panel lost its contents.
    pub fn mark_all_dirty(&mut self) {
        self.mark_rows(0, H);
    }

    /// Hands out the changed rows and marks the buffer clean. The caller
    /// must send them to the display before drawing again.
    pub fn take_dirty(&mut self) -> Option<DirtyRows<'_>> {
        let (start, end) = (self.dirty_start, self.dirty_end);
        if start >= end {
            return None;
        }
        self.dirty_start = 0;
        self.dirty_end = 0;
        let bytes = &self.as_bytes()[usize::from(start) * W * 2..usize::from(end) * W * 2];
        Some(DirtyRows {
            y: start,
            rows: end - start,
            bytes,
        })
    }

    /// Mutable access to rows `rows` in wire format, for renderers that
    /// write pixels one by one (a 3D rasteriser testing depth per pixel
    /// cannot use `fill_solid`). Those rows are marked dirty, so a flush
    /// cannot miss them. Store colours converted with [`wire_color`].
    ///
    /// # Panics
    ///
    /// If `rows` is out of bounds or decreasing, like slice indexing.
    ///
    /// ```
    /// use oxstack_gfx::{Framebuffer, Rgb565, wire_color};
    /// use embedded_graphics::prelude::*;
    ///
    /// let mut fb = Framebuffer::<8, 6>::new();
    /// let red = wire_color(Rgb565::RED);
    /// for row in fb.wire_rows_mut(2..4) {
    ///     row[1..3].fill(red);
    /// }
    /// assert_eq!(fb.pixel(1, 3), Some(Rgb565::RED));
    /// let dirty = fb.take_dirty().unwrap();
    /// assert_eq!((dirty.y, dirty.rows), (2, 2));
    /// ```
    pub fn wire_rows_mut(&mut self, rows: Range<usize>) -> &mut [[u16; W]] {
        // Bounds-check (and panic) before marking anything.
        let _ = &self.rows[rows.clone()];
        if !rows.is_empty() {
            self.mark_rows(rows.start, rows.end);
        }
        &mut self.rows[rows]
    }

    /// The whole buffer in wire format.
    pub fn as_bytes(&self) -> &[u8] {
        let px = self.rows.as_flattened();
        // SAFETY: `u16` has no padding or invalid bit patterns, `u8` has
        // alignment 1, and the byte length is exactly `2 * px.len()`.
        unsafe { core::slice::from_raw_parts(px.as_ptr().cast::<u8>(), px.len() * 2) }
    }

    #[inline]
    fn mark_rows(&mut self, start: usize, end: usize) {
        // Both fit in u16 per `ASSERT_FITS_U16`.
        let (start, end) = (start as u16, end as u16);
        if self.dirty_start >= self.dirty_end {
            self.dirty_start = start;
            self.dirty_end = end;
        } else {
            self.dirty_start = self.dirty_start.min(start);
            self.dirty_end = self.dirty_end.max(end);
        }
    }

    #[inline]
    fn wire(color: Rgb565) -> u16 {
        wire_color(color)
    }
}

/// `color` in the framebuffer's wire format (big-endian RGB565), the value
/// [`Framebuffer::wire_rows_mut`] rows hold.
#[inline]
pub fn wire_color(color: Rgb565) -> u16 {
    color.into_storage().to_be()
}

impl<const W: usize, const H: usize> Default for Framebuffer<W, H> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const W: usize, const H: usize> OriginDimensions for Framebuffer<W, H> {
    fn size(&self) -> Size {
        Size::new(W as u32, H as u32)
    }
}

impl<const W: usize, const H: usize> DrawTarget for Framebuffer<W, H> {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Infallible>
    where
        I: IntoIterator<Item = Pixel<Rgb565>>,
    {
        let (mut lo, mut hi) = (usize::MAX, 0);
        for Pixel(Point { x, y }, color) in pixels {
            // Negative coordinates wrap to huge values and fail the bounds check.
            let (x, y) = (x as usize, y as usize);
            if x < W && y < H {
                self.rows[y][x] = Self::wire(color);
                lo = lo.min(y);
                hi = hi.max(y + 1);
            }
        }
        if lo < hi {
            self.mark_rows(lo, hi);
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Rgb565) -> Result<(), Infallible> {
        let area = area.intersection(&self.bounding_box());
        let Some(br) = area.bottom_right() else {
            return Ok(());
        };
        let (x0, y0) = (area.top_left.x as usize, area.top_left.y as usize);
        let (x1, y1) = (br.x as usize + 1, br.y as usize + 1);
        let raw = Self::wire(color);
        for row in &mut self.rows[y0..y1] {
            row[x0..x1].fill(raw);
        }
        self.mark_rows(y0, y1);
        Ok(())
    }

    fn clear(&mut self, color: Rgb565) -> Result<(), Infallible> {
        self.rows.as_flattened_mut().fill(Self::wire(color));
        self.mark_all_dirty();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_graphics::prelude::*;
    use embedded_graphics::primitives::{Line, PrimitiveStyle};

    type Fb = Framebuffer<8, 6>;

    #[test]
    fn new_is_clean_and_black() {
        let mut fb = Fb::new();
        assert!(fb.take_dirty().is_none());
        assert_eq!(fb.pixel(0, 0), Some(Rgb565::BLACK));
        assert_eq!(fb.pixel(8, 0), None);
    }

    #[test]
    fn pixels_are_stored_big_endian() {
        let mut fb = Fb::new();
        let c = Rgb565::new(0b11111, 0, 0b00001); // 0xF801
        Pixel(Point::new(1, 0), c).draw(&mut fb).unwrap();
        assert_eq!(&fb.as_bytes()[2..4], &[0xF8, 0x01]);
        assert_eq!(fb.pixel(1, 0), Some(c));
    }

    #[test]
    fn dirty_rows_track_changes() {
        let mut fb = Fb::new();
        Line::new(Point::new(0, 2), Point::new(7, 3))
            .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 1))
            .draw(&mut fb)
            .unwrap();
        let d = fb.take_dirty().unwrap();
        assert_eq!((d.y, d.rows, d.bytes.len()), (2, 2, 2 * 8 * 2), "span");
        assert!(fb.take_dirty().is_none(), "take_dirty marks clean");

        Pixel(Point::new(0, 1), Rgb565::RED).draw(&mut fb).unwrap();
        Pixel(Point::new(0, 4), Rgb565::RED).draw(&mut fb).unwrap();
        let d = fb.take_dirty().unwrap();
        assert_eq!((d.y, d.rows), (1, 4), "separate draws merge");
    }

    #[test]
    fn wire_rows_mut_marks_rows_dirty() {
        let mut fb = Fb::new();
        assert!(fb.wire_rows_mut(3..3).is_empty());
        assert!(fb.take_dirty().is_none(), "empty range marks nothing");

        fb.wire_rows_mut(4..5)[0][7] = wire_color(Rgb565::BLUE);
        fb.wire_rows_mut(1..2);
        assert_eq!(fb.pixel(7, 4), Some(Rgb565::BLUE));
        let d = fb.take_dirty().unwrap();
        assert_eq!((d.y, d.rows), (1, 4), "ranges merge");
    }

    #[test]
    fn out_of_bounds_drawing_is_clipped() {
        let mut fb = Fb::new();
        Pixel(Point::new(-1, 0), Rgb565::RED).draw(&mut fb).unwrap();
        Pixel(Point::new(0, 6), Rgb565::RED).draw(&mut fb).unwrap();
        assert!(fb.take_dirty().is_none());

        fb.fill_solid(
            &Rectangle::new(Point::new(6, 4), Size::new(10, 10)),
            Rgb565::GREEN,
        )
        .unwrap();
        assert_eq!(fb.pixel(7, 5), Some(Rgb565::GREEN));
        assert_eq!(fb.pixel(5, 5), Some(Rgb565::BLACK));
        let d = fb.take_dirty().unwrap();
        assert_eq!((d.y, d.rows), (4, 2));

        fb.fill_solid(
            &Rectangle::new(Point::new(20, 20), Size::new(2, 2)),
            Rgb565::GREEN,
        )
        .unwrap();
        assert!(fb.take_dirty().is_none());
    }
}
