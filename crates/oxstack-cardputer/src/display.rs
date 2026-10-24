//! The 1.14" 240×135 ST7789V2 LCD, in landscape with the keyboard below.
//!
//! Drawing goes into a [`Frame`] in RAM; [`Display::flush`] sends only the
//! rows that changed, by DMA straight from the framebuffer (no copy). A full
//! frame is 64 800 bytes, about 13 ms at the 40 MHz SPI clock.

use embassy_time::Delay;
use embedded_hal_bus::spi::{DeviceError, ExclusiveDevice, NoDelay};
use esp_hal::Async;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::peripherals::{DMA_CH0, GPIO33, GPIO34, GPIO35, GPIO36, GPIO37, SPI3};
use esp_hal::spi::Mode;
use esp_hal::spi::master::{Config, Spi, SpiDma};
use esp_hal::time::Rate;
use oxstack_drivers::st7789::{self, M5_ST7789V2_INIT, Panel, Rotation, St7789};
use oxstack_gfx::Framebuffer;
use static_cell::ConstStaticCell;

/// Landscape width in pixels.
pub const WIDTH: usize = 240;
/// Landscape height in pixels.
pub const HEIGHT: usize = 135;

/// A full-screen framebuffer.
pub type Frame = Framebuffer<WIDTH, HEIGHT>;

/// How the panel sits in the ST7789's 240×320 memory, per M5GFX.
pub const PANEL: Panel = Panel {
    width: 135,
    height: 240,
    offset_x: 52,
    offset_y: 40,
    memory_width: 240,
    memory_height: 320,
    bgr: true,
    invert: true,
};

/// Keyboard-below landscape orientation.
pub const ROTATION: Rotation = Rotation::Deg90;

/// SPI clock. GPIO35/36 are routed through the GPIO matrix, not IOMUX,
/// which caps reliable output at 40 MHz (M5GFX uses the same).
pub const SPI_FREQUENCY: Rate = Rate::from_mhz(40);

/// The board's framebuffer, placed in `.bss` (64.8 KB RAM, zero flash).
/// Returns `None` after the first call.
pub fn try_framebuffer() -> Option<&'static mut Frame> {
    static FRAME: ConstStaticCell<Frame> = ConstStaticCell::new(Frame::new());
    FRAME.try_take()
}

/// [`try_framebuffer`], panicking if it was already taken.
pub fn framebuffer() -> &'static mut Frame {
    try_framebuffer().expect("framebuffer already taken")
}

type Bus = ExclusiveDevice<SpiDma<'static, Async>, Output<'static>, NoDelay>;

/// Display errors.
pub type Error = st7789::Error<
    DeviceError<esp_hal::spi::Error, core::convert::Infallible>,
    core::convert::Infallible,
>;

/// SPI3, DMA channel 0, and the LCD's control pins.
#[allow(missing_docs)]
pub struct DisplayResources {
    pub spi: SPI3<'static>,
    pub dma: DMA_CH0<'static>,
    pub sck: GPIO36<'static>,
    pub mosi: GPIO35<'static>,
    pub cs: GPIO37<'static>,
    pub dc: GPIO34<'static>,
    pub rst: GPIO33<'static>,
}

impl DisplayResources {
    /// Resets and initialises the panel. Frame memory holds garbage until the
    /// first flush, and the backlight is a separate resource.
    pub async fn init(self) -> Result<Display, Error> {
        // Bounce buffers, used only for data DMA can't read in place (e.g.
        // constants in flash). The framebuffer is in SRAM and goes direct.
        let rx = esp_hal::dma_rx_buffer!(32).expect("valid DMA buffer");
        let tx = esp_hal::dma_tx_buffer!(256).expect("valid DMA buffer");

        let spi = Spi::new(
            self.spi,
            Config::default()
                .with_frequency(SPI_FREQUENCY)
                .with_mode(Mode::_0),
        )
        .expect("40 MHz mode 0 is a valid SPI configuration")
        .with_sck(self.sck)
        .with_mosi(self.mosi)
        .with_dma(self.dma)
        .with_buffers(rx, tx)
        .into_async();

        let out = OutputConfig::default();
        let cs = Output::new(self.cs, Level::High, out);
        let dc = Output::new(self.dc, Level::Low, out);
        let mut rst = Output::new(self.rst, Level::High, out);

        let mut delay = Delay;
        let Ok(()) = st7789::hard_reset(&mut rst, &mut delay).await;

        let Ok(bus) = ExclusiveDevice::new_no_delay(spi, cs);
        let mut lcd = St7789::new(bus, dc, PANEL, ROTATION);
        lcd.init(M5_ST7789V2_INIT, &mut delay).await?;
        Ok(Display { lcd, _rst: rst })
    }
}

/// The LCD driver.
pub struct Display {
    lcd: St7789<Bus, Output<'static>>,
    /// Held so the reset line stays high.
    _rst: Output<'static>,
}

impl Display {
    /// Sends the framebuffer's changed rows; returns whether anything was sent.
    pub async fn flush(&mut self, frame: &mut Frame) -> Result<bool, Error> {
        let Some(dirty) = frame.take_dirty() else {
            return Ok(false);
        };
        self.lcd.write_rows(dirty.y, dirty.bytes).await?;
        Ok(true)
    }

    /// Turns the panel's output on or off; frame memory is retained.
    pub async fn set_display_on(&mut self, on: bool) -> Result<(), Error> {
        self.lcd.set_display_on(on).await
    }

    /// Enters or leaves panel sleep. Waits 120 ms.
    pub async fn set_sleep(&mut self, sleep: bool) -> Result<(), Error> {
        self.lcd.set_sleep(sleep, &mut Delay).await
    }

    /// The underlying controller driver, for raw commands and windows.
    pub fn controller(&mut self) -> &mut St7789<Bus, Output<'static>> {
        &mut self.lcd
    }
}
