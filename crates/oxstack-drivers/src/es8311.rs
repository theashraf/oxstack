//! Everest ES8311 mono audio codec (one ADC, one DAC) over I²C control.
//!
//! The driver configures the codec as an I²S **slave** with 16-bit samples
//! and its internal clock derived from the bit clock (`MCLK = BCLK × 8`).
//! The host needs no MCLK line, and with 16-bit stereo frames
//! (BCLK = 32 × fs) the codec's clock registers are the same at every
//! supported rate. Only the host's I²S clock sets the sample rate.
//!
//! Power is managed by [`Mode`]. [`Es8311::init`] leaves the codec in
//! [`Mode::Off`] with the DAC muted at its minimum volume, so a fresh codec
//! can never make a sound until the caller explicitly asks for playback.
//!
//! Register values follow M5Stack's M5Unified (MIT) and Espressif's
//! `esp-bsp` `es8311` component (Apache-2.0).

use embedded_hal_async::delay::DelayNs;
use embedded_hal_async::i2c::I2c;

/// I²C address with CE tied low.
pub const ADDRESS_CE_LOW: u8 = 0x18;
/// I²C address with CE tied high.
pub const ADDRESS_CE_HIGH: u8 = 0x19;
/// Values of the `CHD1`/`CHD2` chip-ID registers (`0xFD`, `0xFE`).
pub const CHIP_ID: [u8; 2] = [0x83, 0x11];

/// Sample rates supported with BCLK-derived clocking and 16-bit stereo
/// frames. These are the rates for which Espressif's coefficient table has an
/// `MCLK = 32 × fs` entry; lower rates are left out until verified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[allow(missing_docs)]
pub enum SampleRate {
    Hz22050,
    Hz32000,
    Hz44100,
    Hz48000,
    Hz64000,
    Hz88200,
    Hz96000,
}

impl SampleRate {
    /// The rate in hertz.
    pub const fn hz(self) -> u32 {
        match self {
            SampleRate::Hz22050 => 22_050,
            SampleRate::Hz32000 => 32_000,
            SampleRate::Hz44100 => 44_100,
            SampleRate::Hz48000 => 48_000,
            SampleRate::Hz64000 => 64_000,
            SampleRate::Hz88200 => 88_200,
            SampleRate::Hz96000 => 96_000,
        }
    }
}

/// Which signal paths are powered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Mode {
    /// Everything powered down. Silent.
    Off,
    /// DAC on; ADC off.
    Playback,
    /// Microphone PGA and ADC on; DAC powered down, muted and unclocked.
    /// Silent.
    Record,
    /// Both paths on, as in Espressif's reference init. Not yet verified
    /// on hardware.
    Duplex,
}

impl Mode {
    const fn plays(self) -> bool {
        matches!(self, Mode::Playback | Mode::Duplex)
    }
    const fn records(self) -> bool {
        matches!(self, Mode::Record | Mode::Duplex)
    }
}

/// Analog microphone pre-amplifier gain, 0–30 dB in 3 dB steps. The codec
/// adds a further +24 dB digital `ADC_SCALE` (register `0x16` reset value).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct MicGain(u8);

impl MicGain {
    /// 0 dB (M5Unified's default).
    pub const DB0: Self = Self(0);
    /// Maximum, 30 dB.
    pub const DB30: Self = Self(10);

    /// The nearest step at or below `db`, clamped to 0–30 dB.
    pub const fn from_db(db: u8) -> Self {
        let step = db / 3;
        Self(if step > 10 { 10 } else { step })
    }

    /// The gain in dB.
    pub const fn db(self) -> u8 {
        self.0 * 3
    }
}

/// Digital volume: −95.5 dB to +32 dB in 0.5 dB steps (`0xBF` = 0 dB).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Volume(pub u8);

impl Volume {
    /// The quietest setting (−95.5 dB), effectively silent.
    pub const MIN: Self = Self(0x00);
    /// Unity gain.
    pub const UNITY: Self = Self(0xBF);

    /// From a level in 0.5 dB steps relative to unity, saturating.
    pub const fn from_half_db(half_db: i16) -> Self {
        let raw = 0xBF + half_db;
        Self(if raw < 0 {
            0
        } else if raw > 0xFF {
            0xFF
        } else {
            raw as u8
        })
    }
}

mod reg {
    pub const RESET: u8 = 0x00;
    pub const CLK_MANAGER_01: u8 = 0x01;
    pub const CLK_MANAGER_02: u8 = 0x02;
    pub const CLK_MANAGER_03: u8 = 0x03;
    pub const CLK_MANAGER_04: u8 = 0x04;
    pub const CLK_MANAGER_05: u8 = 0x05;
    pub const CLK_MANAGER_07: u8 = 0x07;
    pub const CLK_MANAGER_08: u8 = 0x08;
    pub const SDP_IN: u8 = 0x09;
    pub const SDP_OUT: u8 = 0x0A;
    pub const SYSTEM_0D: u8 = 0x0D;
    pub const SYSTEM_0E: u8 = 0x0E;
    pub const SYSTEM_12: u8 = 0x12;
    pub const SYSTEM_13: u8 = 0x13;
    pub const SYSTEM_14: u8 = 0x14;
    pub const ADC_17: u8 = 0x17;
    pub const ADC_1C: u8 = 0x1C;
    pub const DAC_31: u8 = 0x31;
    pub const DAC_32: u8 = 0x32;
    pub const DAC_37: u8 = 0x37;
    pub const CHD1: u8 = 0xFD;
    pub const CHD2: u8 = 0xFE;
}

mod val {
    /// `RESET`: reset everything.
    pub const RESET_ALL: u8 = 0x1F;
    /// `RESET`: state machine on, I²S slave.
    pub const CSM_ON_SLAVE: u8 = 0x80;
    /// `CLK_MANAGER_01`: MCLK from BCLK (bit 7) plus clock-enable bits:
    /// MCLK (5), BCLK (4), ADC (3), DAC (2), analog ADC (1), analog DAC (0).
    pub const CLK_FROM_BCLK: u8 = 0x80;
    pub const CLK_COMMON: u8 = 0x30;
    pub const CLK_DAC: u8 = 0x05;
    pub const CLK_ADC: u8 = 0x0A;
    /// `CLK_MANAGER_02`: pre-divide ÷1, pre-multiply ×8 (BCLK 32·fs → 256·fs).
    pub const PRE_DIV1_MUL8: u8 = 0x18;
    /// `CLK_MANAGER_03/04`: single-speed, OSR 0x10 for ADC and DAC.
    pub const OSR: u8 = 0x10;
    /// `SDP_IN`/`SDP_OUT`: I²S format, 16-bit words.
    pub const I2S_16BIT: u8 = 0x0C;
    /// `SYSTEM_0D`: analog circuitry on / off.
    pub const ANALOG_ON: u8 = 0x01;
    pub const ANALOG_OFF: u8 = 0xFC;
    /// `SYSTEM_0E`: PGA + ADC modulator on / off.
    pub const ADC_ON: u8 = 0x02;
    pub const ADC_OFF: u8 = 0x6A;
    /// `SYSTEM_12`: DAC on / off.
    pub const DAC_ON: u8 = 0x00;
    pub const DAC_OFF: u8 = 0x02;
    /// `SYSTEM_13`: bit 4 (HPSW) selects headphone drive (Espressif's
    /// playback value); `0x40` is the reset value. There is **no** output
    /// enable bit: silence comes from DAC power-down, mute, minimum volume
    /// and gated DAC clocks together.
    pub const HP_DRIVE: u8 = 0x10;
    pub const OUTPUT_RESET: u8 = 0x40;
    /// `SYSTEM_14`: analog mic on MIC1P/N; low nibble is PGA gain.
    pub const MIC1_SELECT: u8 = 0x10;
    /// `ADC_1C`: ADC equaliser bypass, DC-offset cancel.
    pub const ADC_EQ_BYPASS_DC_CANCEL: u8 = 0x6A;
    /// `DAC_31`: both DAC mute bits.
    pub const DAC_MUTE: u8 = 0x60;
    /// `DAC_37`: DAC equaliser bypass.
    pub const DAC_EQ_BYPASS: u8 = 0x08;
}

/// ES8311 driver.
pub struct Es8311<I2C> {
    i2c: I2C,
    addr: u8,
    mode: Mode,
    mic_gain: MicGain,
}

impl<I2C: I2c> Es8311<I2C> {
    /// Wraps a bus handle. Call [`init`](Self::init) before anything else.
    pub const fn new(i2c: I2C, addr: u8) -> Self {
        Self {
            i2c,
            addr,
            mode: Mode::Off,
            mic_gain: MicGain::DB0,
        }
    }

    /// Reads `CHD1`/`CHD2`. Expect [`CHIP_ID`]. The chip doesn't
    /// auto-increment on reads, so this reads one register at a time.
    pub async fn chip_id(&mut self) -> Result<[u8; 2], I2C::Error> {
        Ok([self.read(reg::CHD1).await?, self.read(reg::CHD2).await?])
    }

    /// Resets the codec and programs clocking and the 16-bit I²S format.
    /// Leaves it in [`Mode::Off`], DAC muted at [`Volume::MIN`].
    pub async fn init(&mut self, delay: &mut impl DelayNs) -> Result<(), I2C::Error> {
        self.write(reg::RESET, val::RESET_ALL).await?;
        delay.delay_ms(20).await;
        self.write(reg::RESET, 0x00).await?;
        // Mute digitally before anything is powered.
        self.write(reg::DAC_31, val::DAC_MUTE).await?;
        self.write(reg::DAC_32, Volume::MIN.0).await?;
        self.write(reg::RESET, val::CSM_ON_SLAVE).await?;

        self.write(reg::CLK_MANAGER_01, val::CLK_FROM_BCLK).await?;
        self.write(reg::CLK_MANAGER_02, val::PRE_DIV1_MUL8).await?;
        self.write(reg::CLK_MANAGER_03, val::OSR).await?;
        self.write(reg::CLK_MANAGER_04, val::OSR).await?;
        self.write(reg::CLK_MANAGER_05, 0x00).await?;
        self.write(reg::CLK_MANAGER_07, 0x00).await?;
        self.write(reg::CLK_MANAGER_08, 0xFF).await?;
        self.write(reg::SDP_IN, val::I2S_16BIT).await?;
        self.write(reg::SDP_OUT, val::I2S_16BIT).await?;
        self.write(reg::ADC_1C, val::ADC_EQ_BYPASS_DC_CANCEL)
            .await?;
        self.write(reg::DAC_37, val::DAC_EQ_BYPASS).await?;

        self.mode = Mode::Duplex; // force every register to be written
        self.set_mode(Mode::Off).await
    }

    /// Current power mode.
    pub const fn mode(&self) -> Mode {
        self.mode
    }

    /// Powers the requested paths. Silence outside playback modes rests on
    /// four independent controls: DAC power-down (`0x12`), DAC mute (`0x31`),
    /// minimum volume until set (`0x32`), and gated DAC clocks (`0x01`).
    ///
    /// The I²S clock must be running before
    /// entering any mode other than [`Mode::Off`].
    ///
    /// Paths are powered down before others are powered up, and the DAC
    /// mute is left as it is. Call [`set_dac_mute`](Self::set_dac_mute)
    /// after the first samples are flowing to avoid a pop.
    pub async fn set_mode(&mut self, mode: Mode) -> Result<(), I2C::Error> {
        if mode == self.mode {
            return Ok(());
        }
        if !mode.plays() {
            self.write(reg::SYSTEM_13, val::OUTPUT_RESET).await?;
            self.write(reg::SYSTEM_12, val::DAC_OFF).await?;
        }
        if !mode.records() {
            self.write(reg::SYSTEM_0E, val::ADC_OFF).await?;
        }

        let mut clocks = val::CLK_FROM_BCLK;
        if mode != Mode::Off {
            clocks |= val::CLK_COMMON;
        }
        if mode.plays() {
            clocks |= val::CLK_DAC;
        }
        if mode.records() {
            clocks |= val::CLK_ADC;
        }
        self.write(reg::CLK_MANAGER_01, clocks).await?;

        if mode == Mode::Off {
            self.write(reg::SYSTEM_0D, val::ANALOG_OFF).await?;
        } else {
            self.write(reg::SYSTEM_0D, val::ANALOG_ON).await?;
        }
        if mode.records() {
            self.write(reg::SYSTEM_14, val::MIC1_SELECT | self.mic_gain.0)
                .await?;
            self.write(reg::ADC_17, Volume::UNITY.0).await?;
            self.write(reg::SYSTEM_0E, val::ADC_ON).await?;
        }
        if mode.plays() {
            self.write(reg::SYSTEM_12, val::DAC_ON).await?;
            self.write(reg::SYSTEM_13, val::HP_DRIVE).await?;
        }
        self.mode = mode;
        Ok(())
    }

    /// Microphone pre-amplifier gain (applied now if recording).
    pub async fn set_mic_gain(&mut self, gain: MicGain) -> Result<(), I2C::Error> {
        self.mic_gain = gain;
        if self.mode.records() {
            self.write(reg::SYSTEM_14, val::MIC1_SELECT | gain.0)
                .await?;
        }
        Ok(())
    }

    /// Digital ADC volume.
    pub async fn set_adc_volume(&mut self, volume: Volume) -> Result<(), I2C::Error> {
        self.write(reg::ADC_17, volume.0).await
    }

    /// Digital DAC volume.
    pub async fn set_dac_volume(&mut self, volume: Volume) -> Result<(), I2C::Error> {
        self.write(reg::DAC_32, volume.0).await
    }

    /// Soft-mutes the DAC (both mute bits).
    pub async fn set_dac_mute(&mut self, mute: bool) -> Result<(), I2C::Error> {
        self.write(reg::DAC_31, if mute { val::DAC_MUTE } else { 0x00 })
            .await
    }

    /// Returns the bus handle.
    pub fn release(self) -> I2C {
        self.i2c
    }

    async fn write(&mut self, reg: u8, value: u8) -> Result<(), I2C::Error> {
        self.i2c.write(self.addr, &[reg, value]).await
    }

    async fn read(&mut self, reg: u8) -> Result<u8, I2C::Error> {
        let mut b = [0];
        self.i2c.write_read(self.addr, &[reg], &mut b).await?;
        Ok(b[0])
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::test_util::block_on;
    use core::convert::Infallible;
    use embedded_hal_async::i2c::Operation;
    use std::vec::Vec;

    /// Records every register write in order.
    #[derive(Default)]
    struct Log(Vec<(u8, u8)>);

    impl Log {
        fn last(&self, reg: u8) -> Option<u8> {
            self.0
                .iter()
                .rev()
                .find(|(r, _)| *r == reg)
                .map(|(_, v)| *v)
        }
        fn position(&self, reg: u8, value: u8) -> usize {
            self.0.iter().position(|w| *w == (reg, value)).unwrap()
        }
    }

    impl embedded_hal_async::i2c::ErrorType for Log {
        type Error = Infallible;
    }

    impl I2c for Log {
        async fn transaction(
            &mut self,
            addr: u8,
            ops: &mut [Operation<'_>],
        ) -> Result<(), Infallible> {
            assert_eq!(addr, ADDRESS_CE_LOW);
            for op in ops {
                if let Operation::Write([r, v]) = op {
                    self.0.push((*r, *v));
                }
            }
            Ok(())
        }
    }

    struct NoDelay;
    impl DelayNs for NoDelay {
        async fn delay_ns(&mut self, _: u32) {}
    }

    fn init() -> Es8311<Log> {
        let mut c = Es8311::new(Log::default(), ADDRESS_CE_LOW);
        block_on(c.init(&mut NoDelay)).unwrap();
        c
    }

    #[test]
    fn init_ends_silent_and_powered_down() {
        let log = init().release();
        assert_eq!(log.last(reg::DAC_31), Some(val::DAC_MUTE));
        assert_eq!(log.last(reg::DAC_32), Some(0x00));
        assert_eq!(log.last(reg::SYSTEM_12), Some(val::DAC_OFF));
        assert_eq!(log.last(reg::SYSTEM_13), Some(val::OUTPUT_RESET));
        assert_eq!(log.last(reg::SYSTEM_0D), Some(val::ANALOG_OFF));
        // Muting happens before the state machine is switched on.
        assert!(log.position(reg::DAC_32, 0x00) < log.position(reg::RESET, val::CSM_ON_SLAVE));
    }

    #[test]
    fn record_never_touches_the_output_path() {
        let mut c = init();
        let before = c.i2c.0.len();
        block_on(c.set_mode(Mode::Record)).unwrap();
        let log = c.release();
        let writes = &log.0[before..];
        assert!(!writes.contains(&(reg::SYSTEM_12, val::DAC_ON)));
        assert!(!writes.contains(&(reg::SYSTEM_13, val::HP_DRIVE)));
        assert_eq!(log.last(reg::SYSTEM_0E), Some(val::ADC_ON));
    }

    #[test]
    fn clock_bits_match_m5unified_per_mode() {
        for (mode, clocks) in [
            (Mode::Playback, 0xB5),
            (Mode::Record, 0xBA),
            (Mode::Duplex, 0xBF),
        ] {
            let mut c = init();
            block_on(c.set_mode(mode)).unwrap();
            assert_eq!(
                c.release().last(reg::CLK_MANAGER_01),
                Some(clocks),
                "{mode:?}"
            );
        }
    }

    #[test]
    fn gains_and_volumes() {
        assert_eq!(MicGain::from_db(31), MicGain::DB30);
        assert_eq!(MicGain::from_db(7).db(), 6);
        assert_eq!(Volume::from_half_db(0), Volume::UNITY);
        assert_eq!(Volume::from_half_db(-1000), Volume::MIN);
        assert_eq!(Volume::from_half_db(-12).0, 0xB3); // −6 dB
    }
}
