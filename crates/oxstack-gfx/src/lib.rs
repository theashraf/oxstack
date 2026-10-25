//! Graphics primitives for oxstack devices.
//!
//! The core type is [`Framebuffer`]: a full-screen RGB565 buffer that
//! stores every pixel *already in the panel's wire format* (big-endian), and
//! remembers which rows changed since the last flush. Flushing is then a
//! single contiguous slice of the buffer, with no conversion pass and no
//! copy, which DMA can stream directly to the display.
//!
//! It implements `embedded-graphics`' `DrawTarget`, so every font, shape
//! and image crate in that ecosystem works unchanged. Renderers that write
//! pixels one at a time, such as `oxstack-3d`, use
//! [`Framebuffer::wire_rows_mut`] instead.
//!
//! [`TextBuf`] is the small formatting buffer every status line needs:
//! `write!` into it, draw [`as_str`](TextBuf::as_str), nothing allocates.
#![no_std]
#![warn(missing_docs)]

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

mod framebuffer;
mod text;

pub use embedded_graphics_core::pixelcolor::Rgb565;
pub use framebuffer::{DirtyRows, Framebuffer, wire_color};
pub use text::TextBuf;
