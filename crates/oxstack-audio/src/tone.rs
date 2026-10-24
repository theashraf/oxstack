//! Sine-tone generator using a phase accumulator and a sine table built at
//! compile time (256 entries, 512 bytes of flash, no runtime trig).

/// One period of a sine, 256 steps, scaled to ±32767.
static SINE: [i16; 256] = sine_table();

const fn sine_table() -> [i16; 256] {
    let mut t = [0i16; 256];
    let mut i = 0;
    while i < 256 {
        // Taylor series of sin(x) on x in [-π, π], evaluated in f64 at
        // compile time; 11 terms keep the error far below one LSB.
        let mut x = (i as f64) * (2.0 * core::f64::consts::PI / 256.0);
        if x > core::f64::consts::PI {
            x -= 2.0 * core::f64::consts::PI;
        }
        let (mut term, mut sum, mut k) = (x, 0.0, 1);
        while k < 23 {
            sum += term;
            term *= -x * x / (((k + 1) * (k + 2)) as f64);
            k += 2;
        }
        let v = sum * 32767.0;
        t[i] = (if v >= 0.0 { v + 0.5 } else { v - 0.5 }) as i16;
        i += 1;
    }
    t
}

/// A continuous sine oscillator.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Sine {
    phase: u32,
    step: u32,
    amplitude: i32,
}

impl Sine {
    /// A tone of `freq_hz` at `sample_rate`, with `amplitude` in
    /// `0.0..=1.0` of full scale.
    pub fn new(freq_hz: f32, sample_rate: u32, amplitude: f32) -> Self {
        // The phase is a 32-bit fraction of a full turn; the table index is
        // its top 8 bits.
        let step = (freq_hz as f64 / f64::from(sample_rate) * 4_294_967_296.0) as u32;
        let amplitude = (amplitude.clamp(0.0, 1.0) * 32768.0) as i32;
        Self {
            phase: 0,
            step,
            amplitude,
        }
    }

    /// Changes the frequency without resetting the phase, so the waveform
    /// stays continuous (no click).
    pub fn set_frequency(&mut self, freq_hz: f32, sample_rate: u32) {
        self.step = Self::new(freq_hz, sample_rate, 0.0).step;
    }

    /// Next sample.
    pub fn next_sample(&mut self) -> i16 {
        let s = i32::from(SINE[(self.phase >> 24) as usize]);
        self.phase = self.phase.wrapping_add(self.step);
        ((s * self.amplitude) >> 15) as i16
    }

    /// Fills interleaved frames, writing the same sample to every channel.
    pub fn fill(&mut self, frames: &mut [i16], channels: usize) {
        for frame in frames.chunks_exact_mut(channels.max(1)) {
            frame.fill(self.next_sample());
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::level::Level;

    #[test]
    fn table_hits_the_cardinal_points() {
        assert_eq!(SINE[0], 0);
        assert_eq!(SINE[64], 32767);
        assert_eq!(SINE[128], 0);
        assert_eq!(SINE[192], -32767);
    }

    #[test]
    fn full_scale_1khz_stereo() {
        let mut osc = Sine::new(1000.0, 48_000, 1.0);
        let mut buf = [0i16; 2 * 4800];
        osc.fill(&mut buf, 2);
        assert!(buf.chunks(2).all(|f| f[0] == f[1]), "channels differ");

        let l = Level::measure(&buf, 0, 2);
        assert!((l.rms_dbfs() + 3.01).abs() < 0.05, "rms {}", l.rms_dbfs());
        assert!(l.dc.abs() < 1e-3, "dc {}", l.dc);

        // 48 samples per period: 100 periods, 200 zero crossings.
        let left: std::vec::Vec<i16> = buf.iter().step_by(2).copied().collect();
        let crossings = left.windows(2).filter(|w| (w[0] < 0) != (w[1] < 0)).count();
        assert!((199..=201).contains(&crossings), "{crossings} crossings");
    }

    #[test]
    fn frequency_changes_are_continuous() {
        // A 1 kHz -> 2 kHz switch must not jump more than one 2 kHz step.
        let mut osc = Sine::new(1000.0, 48_000, 1.0);
        let mut prev = 0i32;
        for i in 0..2000 {
            if i == 1000 {
                osc.set_frequency(2000.0, 48_000);
            }
            let s = i32::from(osc.next_sample());
            // Max slope of a full-scale 2 kHz sine at 48 kHz is 2π·2000/48000 ≈ 0.26 FS/sample.
            assert!(
                i == 0 || (s - prev).abs() < 9000,
                "jump at {i}: {prev} -> {s}"
            );
            prev = s;
        }
    }
}
