//! Sitronix ST7789 TFT controller (4-wire SPI, RGB565).
//!
//! Small ST7789 panels expose only a window of the controller's 240×320
//! frame memory, so every panel has a native size, a memory offset and a
//! rotation-dependent `MADCTL`. [`Panel`] captures that once, and
//! [`Panel::geometry`] derives the window for any [`Rotation`] at compile
//! time.
//!
//! The driver streams pixel bytes as given. It does no conversion and keeps
//! no buffer, so a framebuffer that already stores big-endian RGB565 can be
//! sent with zero copies (and, on ESP32 targets, straight from RAM via DMA).

use embedded_hal::digital::OutputPin;
use embedded_hal_async::delay::DelayNs;
use embedded_hal_async::spi::SpiDevice;

/// Command opcodes used by the driver.
pub mod cmd {
    #![allow(missing_docs)]
    pub const SWRESET: u8 = 0x01;
    pub const SLPIN: u8 = 0x10;
    pub const SLPOUT: u8 = 0x11;
    pub const NORON: u8 = 0x13;
    pub const INVOFF: u8 = 0x20;
    pub const INVON: u8 = 0x21;
    pub const DISPOFF: u8 = 0x28;
    pub const DISPON: u8 = 0x29;
    pub const CASET: u8 = 0x2A;
    pub const RASET: u8 = 0x2B;
    pub const RAMWR: u8 = 0x2C;
    pub const MADCTL: u8 = 0x36;
    pub const IDMOFF: u8 = 0x38;
    pub const COLMOD: u8 = 0x3A;
}

/// `MADCTL` bits.
pub mod madctl {
    #![allow(missing_docs)]
    pub const MY: u8 = 0x80;
    pub const MX: u8 = 0x40;
    pub const MV: u8 = 0x20;
    pub const ML: u8 = 0x10;
    pub const BGR: u8 = 0x08;
    pub const MH: u8 = 0x04;
}

/// `COLMOD` value for 16 bits per pixel (RGB565) on both interfaces.
const COLMOD_RGB565: u8 = 0x55;

/// One step of a controller initialisation script.
#[derive(Clone, Copy, Debug)]
pub struct InitStep {
    /// Command opcode.
    pub cmd: u8,
    /// Parameter bytes sent with `DC` high.
    pub params: &'static [u8],
    /// Time to wait after the command, in milliseconds.
    pub delay_ms: u16,
}

impl InitStep {
    const fn new(cmd: u8, params: &'static [u8]) -> Self {
        Self {
            cmd,
            params,
            delay_ms: 0,
        }
    }
}

/// Power, voltage and gamma settings for the ST7789V2 panels M5Stack ships
/// (Cardputer, StickC Plus family). Values are from M5GFX's `Panel_ST7789`
/// (MIT, © M5Stack); porch and frame-rate registers are left at reset
/// defaults, as M5GFX does, because they differ between panel lots.
pub const M5_ST7789V2_INIT: &[InitStep] = &[
    InitStep::new(0xB7, &[0x35]),       // GCTRL: gate voltages
    InitStep::new(0xBB, &[0x28]),       // VCOMS
    InitStep::new(0xC0, &[0x0C]),       // LCMCTRL
    InitStep::new(0xC2, &[0x01, 0xFF]), // VDVVRHEN
    InitStep::new(0xC3, &[0x10]),       // VRHS
    InitStep::new(0xC4, &[0x20]),       // VDVSET
    InitStep::new(0xD0, &[0xA4, 0xA1]), // PWCTRL1
    InitStep::new(0xB0, &[0x00, 0xC0]), // RAMCTRL: MSB-first, 65K→262K expansion
    InitStep::new(
        0xE0, // PVGAMCTRL
        &[
            0xD0, 0x00, 0x02, 0x07, 0x0A, 0x28, 0x32, 0x44, 0x42, 0x06, 0x0E, 0x12, 0x14, 0x17,
        ],
    ),
    InitStep::new(
        0xE1, // NVGAMCTRL
        &[
            0xD0, 0x00, 0x02, 0x07, 0x0A, 0x28, 0x31, 0x54, 0x47, 0x0E, 0x1C, 0x17, 0x1B, 0x1E,
        ],
    ),
    InitStep {
        cmd: cmd::SLPOUT,
        params: &[],
        delay_ms: 130,
    },
    InitStep::new(cmd::IDMOFF, &[]),
];

/// Screen rotation, clockwise from the panel's native portrait orientation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Rotation {
    /// Native orientation.
    Deg0,
    /// Quarter turn.
    Deg90,
    /// Half turn.
    Deg180,
    /// Three-quarter turn.
    Deg270,
}

/// How a physical panel sits inside the controller's frame memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Panel {
    /// Visible width in native (portrait) orientation.
    pub width: u16,
    /// Visible height in native orientation.
    pub height: u16,
    /// Column of the first visible pixel in frame memory.
    pub offset_x: u16,
    /// Row of the first visible pixel in frame memory.
    pub offset_y: u16,
    /// Frame memory width (240 for ST7789).
    pub memory_width: u16,
    /// Frame memory height (320 for ST7789).
    pub memory_height: u16,
    /// Panel wires sub-pixels in BGR order.
    pub bgr: bool,
    /// Panel needs colour inversion (`INVON`) for correct colours.
    pub invert: bool,
}

/// The drawable window for one panel + rotation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Geometry {
    /// Width in the rotated orientation.
    pub width: u16,
    /// Height in the rotated orientation.
    pub height: u16,
    /// Frame-memory column of logical x = 0.
    pub col_start: u16,
    /// Frame-memory row of logical y = 0.
    pub row_start: u16,
    /// `MADCTL` register value.
    pub madctl: u8,
}

impl Panel {
    /// Window and `MADCTL` for `rotation`.
    ///
    /// When the scan direction is mirrored on an axis, the visible window
    /// moves to the far end of frame memory on that axis. That is why a
    /// panel offset of 52 can become 53 once rotated.
    pub const fn geometry(&self, rotation: Rotation) -> Geometry {
        use madctl::*;
        let (swap, mirror_col, mirror_row, mut bits) = match rotation {
            Rotation::Deg0 => (false, false, false, 0),
            Rotation::Deg90 => (true, false, true, MV | MX | MH),
            Rotation::Deg180 => (false, true, true, MX | MH | MY | ML),
            Rotation::Deg270 => (true, true, false, MV | MY | ML),
        };
        if self.bgr {
            bits |= BGR;
        }
        let (w, h, ox, oy, mw, mh) = if swap {
            (
                self.height,
                self.width,
                self.offset_y,
                self.offset_x,
                self.memory_height,
                self.memory_width,
            )
        } else {
            (
                self.width,
                self.height,
                self.offset_x,
                self.offset_y,
                self.memory_width,
                self.memory_height,
            )
        };
        Geometry {
            width: w,
            height: h,
            col_start: if mirror_col { mw - (w + ox) } else { ox },
            row_start: if mirror_row { mh - (h + oy) } else { oy },
            madctl: bits,
        }
    }
}

/// Driver error: either the SPI device or the `DC` pin failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error<S, P> {
    /// SPI transfer failed.
    Spi(S),
    /// Driving the data/command pin failed.
    Pin(P),
}

/// Pulses a hardware reset line (active low) and waits for the controller.
pub async fn hard_reset<P: OutputPin, D: DelayNs>(
    rst: &mut P,
    delay: &mut D,
) -> Result<(), P::Error> {
    rst.set_low()?;
    delay.delay_us(20).await;
    rst.set_high()?;
    // Datasheet: up to 120 ms before the controller accepts SLPOUT.
    delay.delay_ms(120).await;
    Ok(())
}

/// ST7789 driver.
pub struct St7789<SPI, DC> {
    spi: SPI,
    dc: DC,
    panel: Panel,
    geometry: Geometry,
}

type LcdResult<T, SPI, DC> = core::result::Result<
    T,
    Error<
        <SPI as embedded_hal_async::spi::ErrorType>::Error,
        <DC as embedded_hal::digital::ErrorType>::Error,
    >,
>;

impl<SPI: SpiDevice, DC: OutputPin> St7789<SPI, DC> {
    /// Wraps an SPI device and the `DC` pin. Call [`init`](Self::init) next.
    pub const fn new(spi: SPI, dc: DC, panel: Panel, rotation: Rotation) -> Self {
        Self {
            spi,
            dc,
            panel,
            geometry: panel.geometry(rotation),
        }
    }

    /// Runs `script`, then sets RGB565, rotation, inversion, and turns the
    /// display on.
    pub async fn init(
        &mut self,
        script: &[InitStep],
        delay: &mut impl DelayNs,
    ) -> LcdResult<(), SPI, DC> {
        for step in script {
            self.command(step.cmd, step.params).await?;
            if step.delay_ms > 0 {
                delay.delay_ms(u32::from(step.delay_ms)).await;
            }
        }
        self.command(cmd::COLMOD, &[COLMOD_RGB565]).await?;
        self.command(cmd::MADCTL, &[self.geometry.madctl]).await?;
        let inv = if self.panel.invert {
            cmd::INVON
        } else {
            cmd::INVOFF
        };
        self.command(inv, &[]).await?;
        self.command(cmd::NORON, &[]).await?;
        self.command(cmd::DISPON, &[]).await
    }

    /// Current drawable window.
    pub const fn geometry(&self) -> Geometry {
        self.geometry
    }

    /// Changes rotation. Frame memory is not redrawn.
    pub async fn set_rotation(&mut self, rotation: Rotation) -> LcdResult<(), SPI, DC> {
        self.geometry = self.panel.geometry(rotation);
        self.command(cmd::MADCTL, &[self.geometry.madctl]).await
    }

    /// Sends a command followed by its parameters.
    pub async fn command(&mut self, cmd: u8, params: &[u8]) -> LcdResult<(), SPI, DC> {
        self.dc.set_low().map_err(Error::Pin)?;
        self.spi.write(&[cmd]).await.map_err(Error::Spi)?;
        if !params.is_empty() {
            self.dc.set_high().map_err(Error::Pin)?;
            self.spi.write(params).await.map_err(Error::Spi)?;
        }
        Ok(())
    }

    /// Selects the inclusive window `(x0, y0)..=(x1, y1)` in logical
    /// coordinates. Coordinates beyond the visible area are the caller's
    /// responsibility.
    pub async fn set_window(
        &mut self,
        x0: u16,
        y0: u16,
        x1: u16,
        y1: u16,
    ) -> LcdResult<(), SPI, DC> {
        let g = self.geometry;
        let [a, b] = (x0 + g.col_start).to_be_bytes();
        let [c, d] = (x1 + g.col_start).to_be_bytes();
        self.command(cmd::CASET, &[a, b, c, d]).await?;
        let [a, b] = (y0 + g.row_start).to_be_bytes();
        let [c, d] = (y1 + g.row_start).to_be_bytes();
        self.command(cmd::RASET, &[a, b, c, d]).await
    }

    /// Streams big-endian RGB565 bytes into the current window.
    pub async fn write_pixels(&mut self, bytes: &[u8]) -> LcdResult<(), SPI, DC> {
        self.command(cmd::RAMWR, &[]).await?;
        self.dc.set_high().map_err(Error::Pin)?;
        self.spi.write(bytes).await.map_err(Error::Spi)
    }

    /// Writes full-width rows starting at row `y`. `bytes` must hold whole
    /// rows; it is sent as one contiguous transfer.
    pub async fn write_rows(&mut self, y: u16, bytes: &[u8]) -> LcdResult<(), SPI, DC> {
        let row_bytes = usize::from(self.geometry.width) * 2;
        debug_assert!(row_bytes > 0 && bytes.len().is_multiple_of(row_bytes));
        let rows = (bytes.len() / row_bytes) as u16;
        if rows == 0 {
            return Ok(());
        }
        self.set_window(0, y, self.geometry.width - 1, y + rows - 1)
            .await?;
        self.write_pixels(bytes).await
    }

    /// Display on/off. Frame memory is kept while off.
    pub async fn set_display_on(&mut self, on: bool) -> LcdResult<(), SPI, DC> {
        self.command(if on { cmd::DISPON } else { cmd::DISPOFF }, &[])
            .await
    }

    /// Enters (`true`) or leaves sleep mode. The controller needs 120 ms
    /// between the two; this call waits that long after the command.
    pub async fn set_sleep(
        &mut self,
        sleep: bool,
        delay: &mut impl DelayNs,
    ) -> LcdResult<(), SPI, DC> {
        self.command(if sleep { cmd::SLPIN } else { cmd::SLPOUT }, &[])
            .await?;
        delay.delay_ms(120).await;
        Ok(())
    }

    /// Software reset; prefer [`hard_reset`] when a reset pin is wired.
    pub async fn soft_reset(&mut self, delay: &mut impl DelayNs) -> LcdResult<(), SPI, DC> {
        self.command(cmd::SWRESET, &[]).await?;
        delay.delay_ms(150).await;
        Ok(())
    }

    /// Returns the SPI device and `DC` pin.
    pub fn release(self) -> (SPI, DC) {
        (self.spi, self.dc)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::test_util::block_on;
    use core::cell::RefCell;
    use core::convert::Infallible;
    use embedded_hal_async::spi::Operation;
    use std::rc::Rc;
    use std::vec::Vec;

    /// The 1.14" panel on the M5Stack Cardputer, as configured by M5GFX.
    const CARDPUTER: Panel = Panel {
        width: 135,
        height: 240,
        offset_x: 52,
        offset_y: 40,
        memory_width: 240,
        memory_height: 320,
        bgr: true,
        invert: true,
    };

    #[test]
    fn cardputer_geometry_per_rotation() {
        let table = [
            (Rotation::Deg0, (135, 240, 52, 40, 0x08)),
            (Rotation::Deg90, (240, 135, 40, 53, 0x6C)),
            (Rotation::Deg180, (135, 240, 53, 40, 0xDC)),
            (Rotation::Deg270, (240, 135, 40, 52, 0xB8)),
        ];
        for (r, expected) in table {
            let g = CARDPUTER.geometry(r);
            assert_eq!(
                (g.width, g.height, g.col_start, g.row_start, g.madctl),
                expected,
                "{r:?}"
            );
            let (mw, mh) = if g.madctl & madctl::MV != 0 {
                (320, 240)
            } else {
                (240, 320)
            };
            assert!(g.col_start + g.width <= mw, "{r:?} overflows columns");
            assert!(g.row_start + g.height <= mh, "{r:?} overflows rows");
        }
    }

    /// `(dc_level, bytes)` for each SPI write.
    type WriteLog = Rc<RefCell<Vec<(bool, Vec<u8>)>>>;

    /// Records each SPI write together with the `DC` level at that moment.
    #[derive(Clone, Default)]
    struct Bus {
        dc: Rc<RefCell<bool>>,
        log: WriteLog,
    }
    struct Dc(Rc<RefCell<bool>>);

    impl embedded_hal::digital::ErrorType for Dc {
        type Error = Infallible;
    }
    impl OutputPin for Dc {
        fn set_low(&mut self) -> Result<(), Infallible> {
            *self.0.borrow_mut() = false;
            Ok(())
        }
        fn set_high(&mut self) -> Result<(), Infallible> {
            *self.0.borrow_mut() = true;
            Ok(())
        }
    }
    impl embedded_hal_async::spi::ErrorType for Bus {
        type Error = Infallible;
    }
    impl SpiDevice for Bus {
        async fn transaction(&mut self, ops: &mut [Operation<'_, u8>]) -> Result<(), Infallible> {
            for op in ops {
                if let Operation::Write(bytes) = op {
                    self.log
                        .borrow_mut()
                        .push((*self.dc.borrow(), bytes.to_vec()));
                }
            }
            Ok(())
        }
    }

    #[test]
    fn write_rows_sets_offset_window_then_streams_data() {
        let bus = Bus::default();
        let dc = Dc(bus.dc.clone());
        let mut lcd = St7789::new(bus.clone(), dc, CARDPUTER, Rotation::Deg90);
        let rows = [0xAB; 240 * 2 * 2];
        block_on(lcd.write_rows(10, &rows)).unwrap();

        let log = bus.log.borrow();
        let expected: [(bool, &[u8]); 6] = [
            (false, &[cmd::CASET]),
            (true, &[0, 40, 1, 23]), // 40..=279
            (false, &[cmd::RASET]),
            (true, &[0, 63, 0, 64]), // 53+10 ..= 53+11
            (false, &[cmd::RAMWR]),
            (true, &rows),
        ];
        assert_eq!(log.len(), expected.len());
        for (got, want) in log.iter().zip(expected) {
            assert_eq!((got.0, got.1.as_slice()), want);
        }
    }
}
