use core::ops::Range;

/// A `W`×`H` depth buffer of `u16`, one value per framebuffer pixel.
///
/// Values are *nearness*: 0 means nothing drawn (infinitely far) and larger
/// means closer, the reverse of the usual convention. That makes the empty
/// buffer all-zero, so [`DepthBuffer::new`] is `const`, lands in `.bss`
/// like the framebuffer, and costs RAM (64.8 KB at 240×135) but **no
/// flash**. Keep it in a `static`.
///
/// Sixteen bits are plenty at this resolution as long as the camera's near
/// plane is not tiny: see [`Projection`](crate::Projection).
pub struct DepthBuffer<const W: usize, const H: usize> {
    rows: [[u16; W]; H],
}

impl<const W: usize, const H: usize> DepthBuffer<W, H> {
    /// An empty buffer.
    pub const fn new() -> Self {
        Self { rows: [[0; W]; H] }
    }

    /// Empties the buffer. Call once per frame, before drawing.
    pub fn clear(&mut self) {
        self.rows.as_flattened_mut().fill(0);
    }

    /// The stored nearness at `(x, y)` (0 = nothing drawn), or `None`
    /// outside the buffer.
    pub fn get(&self, x: usize, y: usize) -> Option<u16> {
        self.rows.get(y)?.get(x).copied()
    }

    pub(crate) fn rows_mut(&mut self, rows: Range<usize>) -> &mut [[u16; W]] {
        &mut self.rows[rows]
    }
}

impl<const W: usize, const H: usize> Default for DepthBuffer<W, H> {
    fn default() -> Self {
        Self::new()
    }
}
