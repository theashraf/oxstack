//! Audio: ES8311 codec on I²S0 (DMA channel 1) and the internal I²C bus,
//! driving an NS4150B speaker amplifier and fed by an analog MEMS mic.
//!
//! Two drivers share the hardware, one at a time:
//! - [`AudioResources::recorder`] captures the microphone. **Silent by
//!   construction**: I²S TX is never set up (DOUT stays unconnected), and
//!   the codec's DAC stays powered down, muted, at minimum volume and
//!   unclocked.
//! - [`AudioResources::player`] plays audio through the speaker (or
//!   headphones, which mute the amplifier in hardware).
//!
//! Both borrow the resources, so a program can record, drop the
//! [`Recorder`], then play, as often as it likes. They also share one
//! 32 KB DMA stream buffer, since only one of them can exist at a time.
//!
//! The codec takes its master clock from the I²S bit clock, so the clock
//! must keep running whenever the codec is powered. Both drivers start the
//! I²S stream *before* powering the codec; [`Recorder::shutdown`] and
//! [`Player::shutdown`] power it down before stopping the stream. Plain
//! `drop` stops the stream at once and leaves the codec for the next `init`
//! to reset.
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
/// Stream buffer size: 32 KB, about 170 ms at 48 kHz.
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
pub struct AudioResources<'d> {
    pub i2s: I2S0<'d>,
    pub dma: DMA_CH1<'d>,
    pub bclk: GPIO41<'d>,
    pub ws: GPIO43<'d>,
    pub dout: GPIO42<'d>,
    pub din: GPIO46<'d>,
}
reborrow_group!(AudioResources {
    i2s,
    dma,
    bclk,
    ws,
    dout,
    din
});

fn i2s_config(rate: SampleRate) -> TdmConfig {
    TdmConfig::new_tdm_philips()
        .with_sample_rate(Rate::from_hz(rate.hz()))
        .with_data_format(DataFormat::Data16Channel16)
        .with_channels(Channels::STEREO)
}

/// The one DMA stream buffer, used for capture or playback. The `&mut`
/// borrow of [`AudioResources`] guarantees a single user at a time.
macro_rules! stream_memory {
    () => {
        static_dma_memory!(STREAM_BYTES, CHUNK)
    };
}

impl<'d> AudioResources<'d> {
    /// Starts capturing the microphone. Never produces sound.
    pub async fn recorder(
        &mut self,
        bus: &'static SysI2cBus,
        rate: SampleRate,
        gain: MicGain,
    ) -> Result<Recorder<'_>, Error> {
        let i2s = I2s::new(self.i2s.reborrow(), self.dma.reborrow(), i2s_config(rate))
            .map_err(Error::Config)?
            .into_async();
        let rx = i2s
            .i2s_rx
            .with_bclk(self.bclk.reborrow())
            .with_ws(self.ws.reborrow())
            .with_din(self.din.reborrow())
            .build();
        let (buffer, descriptors) = stream_memory!();
        let buf = DmaRxStreamBuf::new(descriptors, buffer).expect("valid DMA stream buffer");
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
        })
    }

    /// Starts audio output, muted at `volume` until the first
    /// [`Player::write`].
    pub async fn player(
        &mut self,
        bus: &'static SysI2cBus,
        rate: SampleRate,
        volume: Volume,
    ) -> Result<Player<'_>, Error> {
        let i2s = I2s::new(self.i2s.reborrow(), self.dma.reborrow(), i2s_config(rate))
            .map_err(Error::Config)?
            .into_async();
        let tx = i2s
            .i2s_tx
            .with_bclk(self.bclk.reborrow())
            .with_ws(self.ws.reborrow())
            .with_dout(self.dout.reborrow())
            .build();
        let (mut buffer, descriptors) = stream_memory!();
        // A previous recording may have left samples here: open with silence.
        buffer.fill(0);
        let buf = DmaTxStreamBuf::new(descriptors, buffer).expect("valid DMA stream buffer");
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
        })
    }
}

/// A DMA stream that may need restarting after an overrun or underrun.
enum Stream<T, U, B> {
    Running(T),
    Stopped(U, B),
    Poisoned,
}

type RxStream<'d> =
    Stream<I2sRxDmaTransfer<'d, Async, DmaRxStreamBuf>, I2sRx<'d, Async>, DmaRxStreamBuf>;
type TxStream<'d> =
    Stream<I2sTxDmaTransfer<'d, Async, DmaTxStreamBuf>, I2sTx<'d, Async>, DmaTxStreamBuf>;

/// Microphone capture.
pub struct Recorder<'d> {
    codec: Codec,
    rx: RxStream<'d>,
    rate: SampleRate,
}

impl Recorder<'_> {
    /// Waits for audio and fills `out` with as many whole stereo frames as
    /// are available (at least one). Returns the number of `i16` samples
    /// written, always even.
    ///
    /// Read at least every ~170 ms (at 48 kHz) or the stream overruns; an
    /// overrun returns [`Error::Overrun`] once and restarts the stream.
    ///
    /// **Cancel-safe.** It only waits before copying, so a cancelled call
    /// has written nothing to `out` and dropped no audio: the samples stay
    /// in the DMA buffer for the next call.
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

    /// Powers the codec down, then stops the stream. Prefer this over a
    /// plain `drop` when the device goes quiet for a while.
    pub async fn shutdown(mut self) -> Result<(), Error> {
        self.codec.set_mode(Mode::Off).await?;
        Ok(()) // dropping `self` stops the I²S stream
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
pub struct Player<'d> {
    codec: Codec,
    tx: TxStream<'d>,
    rate: SampleRate,
    muted: bool,
}

impl Player<'_> {
    /// Queues interleaved stereo samples, waiting for space as needed, and
    /// returns once all are queued. Unmutes on first use.
    ///
    /// Keep the queue fed: if it runs dry the stream stops, returns
    /// [`Error::Underrun`] once, and is restarted.
    ///
    /// **Not cancel-safe**: a cancelled call may have queued any prefix of
    /// `samples`, and there is no way to learn how much. Use
    /// [`write_some`](Self::write_some) in a `select`.
    pub async fn write(&mut self, samples: &[i16]) -> Result<(), Error> {
        let mut rest = samples;
        while !rest.is_empty() {
            let n = self.write_some(rest).await?;
            rest = &rest[n..];
        }
        Ok(())
    }

    /// Waits for space, queues as many whole frames from `samples` as fit,
    /// and returns how many `i16` samples were taken (always even, at least
    /// two unless `samples` has fewer). Unmutes on first use.
    ///
    /// **Cancel-safe**: it only waits before queueing, so a cancelled call
    /// has queued nothing.
    pub async fn write_some(&mut self, samples: &[i16]) -> Result<usize, Error> {
        let bytes = as_bytes(samples);
        let bytes = &bytes[..bytes.len() / FRAME_BYTES * FRAME_BYTES];
        if bytes.is_empty() {
            return Ok(0);
        }
        loop {
            let Stream::Running(t) = &mut self.tx else {
                self.restart()?;
                continue;
            };
            let space = t.available_bytes() / FRAME_BYTES * FRAME_BYTES;
            if space > 0 {
                let n = t.push(&bytes[..space.min(bytes.len())]);
                if self.muted {
                    // Cancelled here, `muted` stays true and the next write
                    // unmutes again: harmless.
                    self.codec.set_dac_mute(false).await?;
                    self.muted = false;
                }
                return Ok(n / 2);
            }
            if t.wait_for_available_async().await.is_err() {
                self.restart()?;
                return Err(Error::Underrun);
            }
        }
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

    /// Mutes, powers the codec down, then stops the stream. Prefer this over
    /// a plain `drop`, which can click.
    pub async fn shutdown(mut self) -> Result<(), Error> {
        self.codec.set_dac_mute(true).await?;
        self.codec.set_mode(Mode::Off).await?;
        Ok(()) // dropping `self` stops the I²S stream
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
