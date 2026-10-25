use core::fmt;
use core::ops::Deref;

/// A fixed-capacity, stack-allocated string for formatting text to draw.
///
/// `write!` into it never fails: text that does not fit is dropped at a
/// character boundary and [`truncated`](Self::truncated) is set. That makes
/// it safe for status lines, where a long SSID should not take the whole
/// screen down with it.
///
/// ```
/// use core::fmt::Write as _;
/// use oxstack_gfx::TextBuf;
///
/// let mut line = TextBuf::<16>::new();
/// write!(line, "{:>4} dBm ch {}", -61, 6).unwrap();
/// assert_eq!(line.as_str(), " -61 dBm ch 6");
///
/// let long = oxstack_gfx::text!(8, "{}", "0123456789");
/// assert_eq!(long.as_str(), "01234567");
/// assert!(long.truncated());
/// ```
#[derive(Clone, Copy)]
pub struct TextBuf<const N: usize> {
    buf: [u8; N],
    len: usize,
    truncated: bool,
}

impl<const N: usize> TextBuf<N> {
    /// An empty buffer.
    pub const fn new() -> Self {
        Self {
            buf: [0; N],
            len: 0,
            truncated: false,
        }
    }

    /// Formats `args` into a new buffer: `TextBuf::<40>::format(format_args!(…))`,
    /// or the [`text!`](crate::text) macro.
    pub fn format(args: fmt::Arguments<'_>) -> Self {
        let mut s = Self::new();
        let _ = fmt::Write::write_fmt(&mut s, args);
        s
    }

    /// The text.
    pub fn as_str(&self) -> &str {
        // Only whole UTF-8 characters are ever copied in.
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }

    /// Empties the buffer and clears the truncation flag.
    pub fn clear(&mut self) {
        self.len = 0;
        self.truncated = false;
    }

    /// Length in bytes.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether nothing has been written.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Capacity in bytes.
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Whether anything written since the last [`clear`](Self::clear) did
    /// not fit.
    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// Appends spaces until the text is at least `width` characters long,
    /// so that drawing it with a background colour overwrites whatever
    /// shorter text was there before.
    pub fn pad(&mut self, width: usize) -> &mut Self {
        let chars = self.as_str().chars().count();
        for _ in chars..width {
            if self.len == N {
                break;
            }
            self.buf[self.len] = b' ';
            self.len += 1;
        }
        self
    }
}

impl<const N: usize> Default for TextBuf<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> fmt::Write for TextBuf<N> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let room = N - self.len;
        let take = if s.len() <= room {
            s.len()
        } else {
            self.truncated = true;
            // Back off to a character boundary.
            let mut n = room;
            while !s.is_char_boundary(n) {
                n -= 1;
            }
            n
        };
        self.buf[self.len..self.len + take].copy_from_slice(&s.as_bytes()[..take]);
        self.len += take;
        Ok(())
    }
}

impl<const N: usize> Deref for TextBuf<N> {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl<const N: usize> AsRef<str> for TextBuf<N> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<const N: usize> fmt::Display for TextBuf<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<const N: usize> fmt::Debug for TextBuf<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl<const N: usize> PartialEq<str> for TextBuf<N> {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl<const N: usize> PartialEq<&str> for TextBuf<N> {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

#[cfg(feature = "defmt")]
impl<const N: usize> defmt::Format for TextBuf<N> {
    fn format(&self, f: defmt::Formatter<'_>) {
        defmt::write!(f, "{}", self.as_str())
    }
}

/// Formats into a [`TextBuf`] of the given capacity:
/// `text!(40, "{} dBm", rssi)`.
#[macro_export]
macro_rules! text {
    ($n:literal, $($arg:tt)*) => {
        $crate::TextBuf::<$n>::format(::core::format_args!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use core::fmt::Write as _;

    use super::*;

    #[test]
    fn truncates_at_char_boundary() {
        let mut t = TextBuf::<5>::new();
        write!(t, "ab€cd").unwrap(); // € is 3 bytes: "ab€" is 5 bytes
        assert_eq!(t, "ab€");
        assert!(t.truncated());
        let mut t = TextBuf::<4>::new();
        write!(t, "ab€cd").unwrap(); // only 4 bytes: € doesn't fit
        assert_eq!(t, "ab");
        assert!(t.truncated());
    }

    #[test]
    fn pad_counts_characters() {
        let mut t = text!(8, "é");
        t.pad(3);
        assert_eq!(t, "é  ");
        assert_eq!(t.len(), 4);
        t.pad(100);
        assert_eq!(t.len(), 8);
    }

    #[test]
    fn clear_resets_everything() {
        let mut t = text!(2, "abc");
        assert!(t.truncated());
        t.clear();
        assert!(t.is_empty() && !t.truncated());
        write!(t, "x").unwrap();
        assert_eq!(&*t, "x");
    }
}
