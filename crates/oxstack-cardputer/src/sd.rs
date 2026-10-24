//! microSD card on the shared SPI bus (SPI2: SCK 40, MOSI 14, MISO 39,
//! CS 12), with FAT through `embedded-sdmmc` (re-exported as [`fs`]).
//!
//! [`SpiBusResources::into_sd`](crate::SpiBusResources::into_sd) takes the
//! whole bus for the card and holds the EXT header's chip-select (GPIO5)
//! high, so a module on the header (e.g. the LoRa cap) can never answer on
//! the bus. [`Sd`] implements [`fs::BlockDevice`], so the guard lives exactly
//! as long as the filesystem.
//!
//! `embedded-sdmmc` is blocking: card I/O holds the CPU for its duration
//! (~25 µs per 512-byte block at 20 MHz, plus card latency). An async
//! block layer is future work.

use embassy_time::Delay;
use embedded_hal_bus::spi::ExclusiveDevice;
pub use embedded_sdmmc as fs;
use embedded_sdmmc::{Block, BlockCount, BlockDevice, BlockIdx, SdCard, TimeSource, Timestamp};
use esp_hal::Blocking;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::peripherals::DMA_CH2;
use esp_hal::spi::Mode;
use esp_hal::spi::master::{Config, Spi};
use esp_hal::time::Rate;

use crate::SpiBusResources;

/// Clock while the card initialises (the SD spec caps it at 400 kHz).
pub const INIT_FREQUENCY: Rate = Rate::from_khz(400);
/// Clock after initialisation. Cards allow 25 MHz in SPI mode; 20 MHz
/// leaves margin for the GPIO-matrix routing of these pins.
pub const FREQUENCY: Rate = Rate::from_mhz(20);

/// The SPI device the card is driven through.
pub type SdSpi = ExclusiveDevice<Spi<'static, Blocking>, Output<'static>, Delay>;
/// The card driver.
pub type Card = SdCard<SdSpi, Delay>;
/// Card errors.
pub type Error = embedded_sdmmc::SdCardError;

fn spi_config(frequency: Rate) -> Config {
    Config::default()
        .with_frequency(frequency)
        .with_mode(Mode::_0)
}

impl SpiBusResources {
    /// Initialises the microSD card, giving it the shared bus exclusively.
    /// Fails if no card is inserted or it doesn't respond.
    pub fn into_sd(self) -> Result<Sd, Error> {
        let out = OutputConfig::default();
        // Deselect both devices before the bus starts clocking.
        let ext_cs = Output::new(self.ext_cs, Level::High, out);
        let sd_cs = Output::new(self.sd_cs, Level::High, out);
        let mut spi = Spi::new(self.spi, spi_config(INIT_FREQUENCY))
            .expect("400 kHz mode 0 is a valid SPI configuration")
            .with_sck(self.sck)
            .with_mosi(self.mosi)
            .with_miso(self.miso);

        // The card enters SPI mode after ≥74 clocks with CS high (sent
        // here, outside any device transaction, so CS stays deasserted).
        spi.write(&[0xFF; 10]).map_err(|_| Error::Transport)?;

        let Ok(dev) = ExclusiveDevice::new(spi, sd_cs, Delay);
        let card = SdCard::new(dev, Delay);
        card.num_bytes()?; // forces initialisation at the slow clock
        card.spi(|dev| dev.bus_mut().apply_config(&spi_config(FREQUENCY)))
            .expect("20 MHz mode 0 is a valid SPI configuration");
        Ok(Sd {
            card,
            _ext_cs: ext_cs,
            _dma: self.dma,
        })
    }
}

/// An initialised microSD card.
pub struct Sd {
    card: Card,
    /// Holds the EXT header deselected while the card owns the bus.
    _ext_cs: Output<'static>,
    /// Reserved for a future DMA block layer.
    _dma: DMA_CH2<'static>,
}

impl Sd {
    /// Card capacity in bytes.
    pub fn size_bytes(&self) -> Result<u64, Error> {
        self.card.num_bytes()
    }

    /// Card generation (SD1, SD2, SDHC/SDXC).
    pub fn card_type(&self) -> Option<embedded_sdmmc::embedded_sdmmc_types::sdcard::CardType> {
        self.card.get_card_type()
    }

    /// The underlying driver.
    pub fn card(&self) -> &Card {
        &self.card
    }
}

impl BlockDevice for Sd {
    type Error = Error;

    fn read(&self, blocks: &mut [Block], start: BlockIdx) -> Result<(), Error> {
        self.card.read(blocks, start)
    }

    fn write(&self, blocks: &[Block], start: BlockIdx) -> Result<(), Error> {
        self.card.write(blocks, start)
    }

    fn num_blocks(&self) -> Result<BlockCount, Error> {
        self.card.num_blocks()
    }
}

/// A [`TimeSource`] for boards without a real-time clock: every timestamp
/// is 2026-01-01 00:00:00. Swap in a real clock (e.g. after network time
/// sync) for meaningful file dates.
#[derive(Clone, Copy, Debug, Default)]
pub struct FixedTime;

impl TimeSource for FixedTime {
    fn get_timestamp(&self) -> Timestamp {
        Timestamp::from_calendar(2026, 1, 1, 0, 0, 0).expect("valid date")
    }
}
