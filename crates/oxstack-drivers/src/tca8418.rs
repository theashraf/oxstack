//! TI TCA8418 I²C keypad scanner.
//!
//! The chip scans a key matrix of up to 8 rows × 10 columns on its own and
//! queues press/release events in a 10-deep FIFO, pulling `INT` low while
//! events are pending. The host only has to drain the FIFO, so a keyboard
//! costs nothing until a key actually changes.
//!
//! ```ignore
//! let mut kp = Tca8418::new(i2c, DEFAULT_ADDRESS);
//! kp.configure(Matrix::new(7, 8)).await?;
//! loop {
//!     int.wait_for_low().await?;
//!     let mut fifo = [0; FIFO_DEPTH];
//!     for raw in kp.drain(&mut fifo).await? {
//!         if let Some(Event::Key(k)) = Event::decode(*raw) { /* ... */ }
//!     }
//!     kp.clear_interrupts().await?;
//! }
//! ```

use embedded_hal_async::i2c::I2c;

/// Factory-fixed 7-bit I²C address.
pub const DEFAULT_ADDRESS: u8 = 0x34;

/// Depth of the on-chip event FIFO.
pub const FIFO_DEPTH: usize = 10;

mod reg {
    pub const CFG: u8 = 0x01;
    pub const INT_STAT: u8 = 0x02;
    pub const KEY_LCK_EC: u8 = 0x03;
    pub const KEY_EVENT_A: u8 = 0x04;
    pub const KP_GPIO1: u8 = 0x1D;
    pub const KP_GPIO2: u8 = 0x1E;
    pub const KP_GPIO3: u8 = 0x1F;
}

mod cfg {
    /// Key-event interrupt enable.
    pub const KE_IEN: u8 = 0x01;
    /// Re-assert INT 50 µs after a clear if events are still pending, so a
    /// host that clears the flag early still sees a fresh edge.
    pub const INT_CFG: u8 = 0x10;
}

/// All interrupt-status bits; each is write-1-to-clear.
const INT_STAT_ALL: u8 = 0x1F;

/// The rows × columns window of the chip's pins used as a key matrix.
///
/// Construct it in a `const` so a bad size is a compile error:
/// `const KEYS: Matrix = Matrix::new(7, 8);`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Matrix {
    rows: u8,
    cols: u8,
}

impl Matrix {
    /// A matrix on `ROW0..rows` × `COL0..cols`.
    ///
    /// # Panics
    /// If `rows` is not in `1..=8` or `cols` is not in `1..=10`
    /// (at compile time when used in a `const`).
    pub const fn new(rows: u8, cols: u8) -> Self {
        assert!(rows >= 1 && rows <= 8, "TCA8418 has 8 row pins");
        assert!(cols >= 1 && cols <= 10, "TCA8418 has 10 column pins");
        Self { rows, cols }
    }

    /// Values for the `KP_GPIO1..3` pin-function registers.
    const fn kp_gpio(self) -> [u8; 3] {
        let rows = ((1u16 << self.rows) - 1) as u8;
        let cols = (1u16 << self.cols) - 1;
        [rows, cols as u8, (cols >> 8) as u8]
    }
}

/// A key press or release at a position in the scan matrix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct KeyEvent {
    /// Row pin (`ROW0..ROW7`).
    pub row: u8,
    /// Column pin (`COL0..COL9`).
    pub col: u8,
    /// `true` on press, `false` on release.
    pub pressed: bool,
}

/// One entry popped from the event FIFO.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Event {
    /// A matrix key changed state.
    Key(KeyEvent),
    /// A pin configured as a general-purpose input changed level.
    Gpi {
        /// Pin index: 0..=7 are `ROW0..7`, 8..=17 are `COL0..9`.
        pin: u8,
        /// `true` when the input became active.
        active: bool,
    },
}

impl Event {
    /// Decodes a raw `KEY_EVENT_A` byte: bit 7 is press/release, bits 0..6
    /// are the key code — `1..=80` for matrix keys (`row * 10 + col + 1`)
    /// and `97..=114` for GPI events. Returns `None` for the empty-FIFO
    /// marker `0` and any reserved code.
    pub const fn decode(raw: u8) -> Option<Self> {
        let pressed = raw & 0x80 != 0;
        match raw & 0x7F {
            code @ 1..=80 => {
                let n = code - 1;
                Some(Event::Key(KeyEvent {
                    row: n / 10,
                    col: n % 10,
                    pressed,
                }))
            }
            code @ 97..=114 => Some(Event::Gpi {
                pin: code - 97,
                active: pressed,
            }),
            _ => None,
        }
    }
}

/// TCA8418 driver.
pub struct Tca8418<I2C> {
    i2c: I2C,
    addr: u8,
}

impl<I2C: I2c> Tca8418<I2C> {
    /// Wraps a bus handle. No I/O happens until [`configure`](Self::configure).
    pub const fn new(i2c: I2C, addr: u8) -> Self {
        Self { i2c, addr }
    }

    /// Puts `matrix` into keypad mode, discards stale events, and enables
    /// the key-event interrupt.
    pub async fn configure(&mut self, matrix: Matrix) -> Result<(), I2C::Error> {
        let [g1, g2, g3] = matrix.kp_gpio();
        self.write(reg::KP_GPIO1, g1).await?;
        self.write(reg::KP_GPIO2, g2).await?;
        self.write(reg::KP_GPIO3, g3).await?;
        // Events captured before configuration (e.g. at power-up) are noise.
        let mut scratch = [0; FIFO_DEPTH];
        self.drain(&mut scratch).await?;
        self.clear_interrupts().await?;
        self.write(reg::CFG, cfg::KE_IEN | cfg::INT_CFG).await
    }

    /// Number of events waiting in the FIFO.
    pub async fn pending(&mut self) -> Result<u8, I2C::Error> {
        Ok(self.read(reg::KEY_LCK_EC).await? & 0x0F)
    }

    /// Pops every pending event into `buf` and returns the filled prefix.
    /// Feed each byte to [`Event::decode`].
    pub async fn drain<'b>(
        &mut self,
        buf: &'b mut [u8; FIFO_DEPTH],
    ) -> Result<&'b [u8], I2C::Error> {
        let count = usize::from(self.pending().await?).min(FIFO_DEPTH);
        for slot in &mut buf[..count] {
            // Each read of KEY_EVENT_A pops the FIFO head.
            *slot = self.read(reg::KEY_EVENT_A).await?;
        }
        Ok(&buf[..count])
    }

    /// Acknowledges all interrupt sources, releasing `INT` if the FIFO is empty.
    pub async fn clear_interrupts(&mut self) -> Result<(), I2C::Error> {
        self.write(reg::INT_STAT, INT_STAT_ALL).await
    }

    /// Returns the bus handle.
    pub fn release(self) -> I2C {
        self.i2c
    }

    async fn write(&mut self, reg: u8, value: u8) -> Result<(), I2C::Error> {
        self.i2c.write(self.addr, &[reg, value]).await
    }

    async fn read(&mut self, reg: u8) -> Result<u8, I2C::Error> {
        let mut out = [0];
        self.i2c.write_read(self.addr, &[reg], &mut out).await?;
        Ok(out[0])
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::test_util::block_on;
    use embedded_hal_async::i2c::{ErrorType, Operation};
    use std::collections::VecDeque;
    use std::vec::Vec;

    /// Register-level model of the chip: a register file plus a FIFO behind
    /// `KEY_EVENT_A` and an event count in `KEY_LCK_EC`.
    #[derive(Default)]
    struct FakeChip {
        regs: [u8; 32],
        fifo: VecDeque<u8>,
        writes: Vec<(u8, u8)>,
    }

    impl ErrorType for FakeChip {
        type Error = core::convert::Infallible;
    }

    impl I2c for FakeChip {
        async fn transaction(
            &mut self,
            addr: u8,
            ops: &mut [Operation<'_>],
        ) -> Result<(), Self::Error> {
            assert_eq!(addr, DEFAULT_ADDRESS);
            let mut ptr = 0u8;
            for op in ops {
                match op {
                    Operation::Write(bytes) => {
                        ptr = bytes[0];
                        if let [reg, value] = bytes[..] {
                            self.writes.push((reg, value));
                            self.regs[usize::from(reg)] = value;
                        }
                    }
                    Operation::Read(out) => {
                        out[0] = match ptr {
                            reg::KEY_LCK_EC => self.fifo.len() as u8,
                            reg::KEY_EVENT_A => self.fifo.pop_front().unwrap_or(0),
                            r => self.regs[usize::from(r)],
                        };
                    }
                }
            }
            Ok(())
        }
    }

    #[test]
    fn decode_key_codes() {
        // Code 1 is ROW0/COL0; code 80 is ROW7/COL9.
        assert_eq!(
            Event::decode(0x81),
            Some(Event::Key(KeyEvent {
                row: 0,
                col: 0,
                pressed: true
            }))
        );
        assert_eq!(
            Event::decode(80),
            Some(Event::Key(KeyEvent {
                row: 7,
                col: 9,
                pressed: false
            }))
        );
        assert_eq!(
            Event::decode(0x80 | 23),
            Some(Event::Key(KeyEvent {
                row: 2,
                col: 2,
                pressed: true
            }))
        );
        assert_eq!(
            Event::decode(0x80 | 97),
            Some(Event::Gpi {
                pin: 0,
                active: true
            })
        );
        assert_eq!(Event::decode(0), None);
        assert_eq!(Event::decode(85), None);
    }

    #[test]
    fn matrix_pin_masks() {
        assert_eq!(Matrix::new(7, 8).kp_gpio(), [0x7F, 0xFF, 0x00]);
        assert_eq!(Matrix::new(8, 10).kp_gpio(), [0xFF, 0xFF, 0x03]);
        assert_eq!(Matrix::new(1, 9).kp_gpio(), [0x01, 0xFF, 0x01]);
    }

    #[test]
    #[should_panic]
    fn matrix_rejects_oversize() {
        let _ = Matrix::new(9, 8);
    }

    #[test]
    fn configure_flushes_stale_events_then_enables_irq() {
        let mut chip = FakeChip::default();
        chip.fifo.extend([0x81, 0x01]);
        let mut kp = Tca8418::new(chip, DEFAULT_ADDRESS);
        block_on(kp.configure(Matrix::new(7, 8))).unwrap();
        let chip = kp.release();

        assert!(chip.fifo.is_empty(), "stale events must be discarded");
        assert_eq!(
            chip.writes,
            [
                (reg::KP_GPIO1, 0x7F),
                (reg::KP_GPIO2, 0xFF),
                (reg::KP_GPIO3, 0x00),
                (reg::INT_STAT, INT_STAT_ALL),
                (reg::CFG, cfg::KE_IEN | cfg::INT_CFG),
            ]
        );
    }

    #[test]
    fn drain_returns_events_in_fifo_order() {
        let mut chip = FakeChip::default();
        chip.fifo.extend([0x81, 0x8C, 0x01]);
        let mut kp = Tca8418::new(chip, DEFAULT_ADDRESS);
        let mut buf = [0; FIFO_DEPTH];
        let events = block_on(kp.drain(&mut buf)).unwrap();
        assert_eq!(events, [0x81, 0x8C, 0x01]);
        assert_eq!(block_on(kp.pending()).unwrap(), 0);
    }
}
