//! Audio: ES8311 codec on I²S0 (DMA channel 1) and the internal I²C bus,
//! driving an NS4150B speaker amplifier and fed by an analog MEMS mic.
//!
//! Two entry points own the hardware exclusively:
//! - [`AudioResources::into_recorder`] captures the microphone. **Silent by
//!   construction**: I²S TX is never set up (DOUT stays unconnected), and
//!   the codec's DAC stays powered down, muted, at minimum volume and
//!   unclocked.
//! - [`AudioResources::into_player`] plays audio through the speaker (or
//!   headphones, which mute the amplifier in hardware).
//!
//! The codec takes its master clock from the I²S bit clock, so the clock
//! must keep running whenever the codec is powered. Both types start the
//! I²S stream *before* powering the codec and power it down before
//! stopping.
//!
//! Samples are interleaved 16-bit stereo (`L R L R …`). The ES8311 has one
//! ADC, so a recording carries the microphone in the left slot; the right
//! slot mirrors it.

use embassy_time::Delay;
use esp_hal::Async;
use esp_hal::dma::{DmaError, DmaRxStreamBuf, DmaTxStreamBuf};
use esp_hal::i2s::master::{
    Channels, DataFormat, I2s, I2sRx, I2sRxDmaTransfer, I2sTx, I2sTxDmaTransfer, TdmConfig,
};
use esp_hal::peripherals::{DMA_CH1, GPIO41, GPIO42, GPIO43, GPIO46, I2S0};
use esp_hal::time::Rate;
use oxstack_drivers::es8311::{ADDRESS_CE_LOW, Es8311, Mode};
pub use oxstack_drivers::es8311::{MicGain, SampleRate, Volume};

use crate::i2c::{SysI2cBus, SysI2cDevice, device};

/// Bytes per stereo 16-bit frame.
pub const FRAME_BYTES: usize = 4;
/// DMA chunk size requested from esp-hal (descriptors end up 2046 bytes).
///
/// Measured on hardware: with 4092-byte descriptors in a 16 KB buffer the
/// RX stream stalled permanently after ~8 KB, right where the I²S unit's
/// 12-bit `RX_EOF_NUM` (which esp-hal sets to its maximum) fires EOF. With
/// this geometry, the one esp-hal's own `embassy_i2s_read` QA test uses, it
/// streams continuously. The exact GDMA interaction is not yet understood;
/// change these values only with an on-device test.
const CHUNK: usize = 2048;
/// Stream buffer size per direction: 32 KB, about 170 ms at 48 kHz.
const STREAM_BYTES: usize = 8 * 4092;

/// The codec on the shared bus.
pub type Codec = Es8311<SysI2cDevice>;

/// Audio errors.
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// Codec control failed.
    I2c(embassy_embedded_hal::shared_bus::I2cDeviceError<esp_hal::i2c::master::Error>),
    /// I²S configuration was rejected.
    Config(esp_hal::i2s::master::ConfigError),
    /// I²S start failed.
    I2s(esp_hal::i2s::master::Error),
    /// DMA failure.
    Dma(DmaError),
    /// Samples were not read in time and some were lost. The stream has
    /// been restarted; the next read continues with fresh audio.
    Overrun,
    /// The output ran dry before more samples arrived. The stream has been
    /// restarted.
    Underrun,
}

impl From<embassy_embedded_hal::shared_bus::I2cDeviceError<esp_hal::i2c::master::Error>> for Error {
    fn from(
        e: embassy_embedded_hal::shared_bus::I2cDeviceError<esp_hal::i2c::master::Error>,
    ) -> Self {
        Error::I2c(e)
    }
}

/// I²S0, DMA channel 1 and the codec link pins.
#[allow(missing_docs)]
pub struct AudioResources {
    pub i2s: I2S0<'static>,
    pub dma: DMA_CH1<'static>,
    pub bclk: GPIO41<'static>,
    pub ws: GPIO43<'static>,
    pub dout: GPIO42<'static>,
    pub din: GPIO46<'static>,
}

fn i2s_config(rate: SampleRate) -> TdmConfig {
    TdmConfig::new_tdm_philips()
        .with_sample_rate(Rate::from_hz(rate.hz()))
        .with_data_format(DataFormat::Data16Channel16)
        .with_channels(Channels::STEREO)
}

impl AudioResources {
    /// Starts capturing the microphone. Never produces sound.
    pub async fn into_recorder(
        self,
        bus: &'static SysI2cBus,
        rate: SampleRate,
        gain: MicGain,
    ) -> Result<Recorder, Error> {
        let i2s = I2s::new(self.i2s, self.dma, i2s_config(rate))
            .map_err(Error::Config)?
            .into_async();
        let rx = i2s
            .i2s_rx
            .with_bclk(self.bclk)
            .with_ws(self.ws)
            .with_din(self.din)
            .build();
        let buf = esp_hal::dma_rx_stream_buffer!(STREAM_BYTES, CHUNK);
        // Clocks first: the codec runs from BCLK.
        let transfer = rx.read(buf).map_err(|(e, _, _)| Error::I2s(e))?;

        let mut codec = Es8311::new(device(bus), ADDRESS_CE_LOW);
        codec.init(&mut Delay).await?;
        codec.set_mic_gain(gain).await?;
        codec.set_mode(Mode::Record).await?;
        Ok(Recorder {
            codec,
            rx: Stream::Running(transfer),
            rate,
            _dout: self.dout,
        })
    }

    /// Starts audio output, muted at `volume` until the first
    /// [`Player::write`].
    pub async fn into_player(
        self,
        bus: &'static SysI2cBus,
        rate: SampleRate,
        volume: Volume,
    ) -> Result<Player, Error> {
        let i2s = I2s::new(self.i2s, self.dma, i2s_config(rate))
            .map_err(Error::Config)?
            .into_async();
        let tx = i2s
            .i2s_tx
            .with_bclk(self.bclk)
            .with_ws(self.ws)
            .with_dout(self.dout)
            .build();
        // The buffer starts zeroed, so the stream opens with silence.
        let buf = esp_hal::dma_tx_stream_buffer!(STREAM_BYTES, CHUNK);
        let transfer = tx.write(buf).map_err(|(e, _, _)| Error::I2s(e))?;

        let mut codec = Es8311::new(device(bus), ADDRESS_CE_LOW);
        codec.init(&mut Delay).await?;
        codec.set_mode(Mode::Playback).await?;
        codec.set_dac_volume(volume).await?;
        Ok(Player {
            codec,
            tx: Stream::Running(transfer),
            rate,
            muted: true,
            _din: self.din,
        })
    }
}

/// A DMA stream that may need restarting after an overrun or underrun.
enum Stream<T, U, B> {
    Running(T),
    Stopped(U, B),
    Poisoned,
}

type RxStream =
    Stream<I2sRxDmaTransfer<'static, Async, DmaRxStreamBuf>, I2sRx<'static, Async>, DmaRxStreamBuf>;
type TxStream =
    Stream<I2sTxDmaTransfer<'static, Async, DmaTxStreamBuf>, I2sTx<'static, Async>, DmaTxStreamBuf>;

/// Microphone capture.
pub struct Recorder {
    codec: Codec,
    rx: RxStream,
    rate: SampleRate,
    /// Owned so nothing else can drive the codec's DAC input.
    _dout: GPIO42<'static>,
}

impl Recorder {
    /// Waits for audio and fills `out` with as many whole stereo frames as
    /// are available (at least one). Returns the number of `i16` samples
    /// written, always even.
    ///
    /// Read at least every ~170 ms (at 48 kHz) or the stream overruns; an
    /// overrun returns [`Error::Overrun`] once and restarts the stream.
    pub async fn read(&mut self, out: &mut [i16]) -> Result<usize, Error> {
        let want = out.len() / 2 * FRAME_BYTES;
        if want == 0 {
            return Ok(0);
        }
        loop {
            let Stream::Running(t) = &mut self.rx else {
                self.restart()?;
                continue;
            };
            let avail = t.available_bytes();
            if avail >= FRAME_BYTES {
                let n = avail.min(want) / FRAME_BYTES * FRAME_BYTES;
                let popped = t.pop(as_bytes_mut(out, n));
                return Ok(popped / 2);
            }
            if t.wait_for_available_async().await.is_err() {
                self.restart()?;
                return Err(Error::Overrun);
            }
        }
    }

    /// Bytes captured but not yet read (0 while the stream is restarting).
    pub fn buffered_bytes(&mut self) -> usize {
        match &mut self.rx {
            Stream::Running(t) => t.available_bytes(),
            _ => 0,
        }
    }

    /// Whether the DMA has stopped (e.g. after an overrun).
    pub fn is_stopped(&mut self) -> bool {
        match &mut self.rx {
            Stream::Running(t) => t.is_done(),
            _ => true,
        }
    }

    /// Changes the microphone pre-amplifier gain.
    pub async fn set_gain(&mut self, gain: MicGain) -> Result<(), Error> {
        Ok(self.codec.set_mic_gain(gain).await?)
    }

    /// The sample rate.
    pub fn rate(&self) -> SampleRate {
        self.rate
    }

    /// The codec, for advanced control.
    pub fn codec(&mut self) -> &mut Codec {
        &mut self.codec
    }

    fn restart(&mut self) -> Result<(), Error> {
        let (rx, buf) = match core::mem::replace(&mut self.rx, Stream::Poisoned) {
            Stream::Running(t) => t.stop(),
            Stream::Stopped(rx, buf) => (rx, buf),
            Stream::Poisoned => return Err(Error::Dma(DmaError::DescriptorError)),
        };
        match rx.read(buf) {
            Ok(t) => {
                self.rx = Stream::Running(t);
                Ok(())
            }
            Err((e, rx, buf)) => {
                self.rx = Stream::Stopped(rx, buf);
                Err(Error::I2s(e))
            }
        }
    }
}

/// Audio output.
pub struct Player {
    codec: Codec,
    tx: TxStream,
    rate: SampleRate,
    muted: bool,
    /// Owned so nothing else drives the codec's ADC output line.
    _din: GPIO46<'static>,
}

impl Player {
    /// Queues interleaved stereo samples, waiting for space as needed, and
    /// returns once all are queued. Unmutes on first use.
    ///
    /// Keep the queue fed: if it runs dry the stream stops, returns
    /// [`Error::Underrun`] once, and is restarted.
    pub async fn write(&mut self, samples: &[i16]) -> Result<(), Error> {
        let mut bytes = as_bytes(samples);
        bytes = &bytes[..bytes.len() / FRAME_BYTES * FRAME_BYTES];
        while !bytes.is_empty() {
            let Stream::Running(t) = &mut self.tx else {
                self.restart()?;
                continue;
            };
            let space = t.available_bytes() / FRAME_BYTES * FRAME_BYTES;
            if space > 0 {
                let n = t.push(&bytes[..space.min(bytes.len())]);
                bytes = &bytes[n..];
                if self.muted {
                    self.codec.set_dac_mute(false).await?;
                    self.muted = false;
                }
                continue;
            }
            if t.wait_for_available_async().await.is_err() {
                self.restart()?;
                return Err(Error::Underrun);
            }
        }
        Ok(())
    }

    /// Digital output volume.
    pub async fn set_volume(&mut self, volume: Volume) -> Result<(), Error> {
        Ok(self.codec.set_dac_volume(volume).await?)
    }

    /// Soft mute.
    pub async fn set_mute(&mut self, mute: bool) -> Result<(), Error> {
        self.codec.set_dac_mute(mute).await?;
        self.muted = mute;
        Ok(())
    }

    /// The sample rate.
    pub fn rate(&self) -> SampleRate {
        self.rate
    }

    /// The codec, for advanced control.
    pub fn codec(&mut self) -> &mut Codec {
        &mut self.codec
    }

    fn restart(&mut self) -> Result<(), Error> {
        let (tx, buf) = match core::mem::replace(&mut self.tx, Stream::Poisoned) {
            Stream::Running(t) => t.stop(),
            Stream::Stopped(tx, buf) => (tx, buf),
            Stream::Poisoned => return Err(Error::Dma(DmaError::DescriptorError)),
        };
        match tx.write(buf) {
            Ok(t) => {
                self.tx = Stream::Running(t);
                Ok(())
            }
            Err((e, tx, buf)) => {
                self.tx = Stream::Stopped(tx, buf);
                Err(Error::I2s(e))
            }
        }
    }
}

fn as_bytes(samples: &[i16]) -> &[u8] {
    // SAFETY: i16 has no padding and any byte pattern is a valid u8; u8 has
    // alignment 1; the length is exactly the slice's size in bytes.
    unsafe { core::slice::from_raw_parts(samples.as_ptr().cast(), samples.len() * 2) }
}

fn as_bytes_mut(samples: &mut [i16], len: usize) -> &mut [u8] {
    debug_assert!(len <= samples.len() * 2);
    // SAFETY: as in `as_bytes`, and every byte pattern is also a valid i16,
    // so writes through the byte view can't create invalid values.
    unsafe { core::slice::from_raw_parts_mut(samples.as_mut_ptr().cast(), len) }
}
