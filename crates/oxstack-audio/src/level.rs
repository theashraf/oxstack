//! Signal level measurement.

/// Peak and RMS level of a block of samples, relative to full scale.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Level {
    /// Largest absolute sample, `0.0..=1.0` of full scale.
    pub peak: f32,
    /// Root-mean-square, `0.0..=1.0` of full scale.
    pub rms: f32,
    /// Mean sample value (DC offset), `-1.0..=1.0` of full scale.
    pub dc: f32,
}

/// Full scale for signed 16-bit PCM (|i16::MIN|).
const FULL_SCALE: f32 = 32768.0;

impl Level {
    /// Measures every `stride`-th sample starting at `offset`. Use
    /// `(0, 2)` for the left channel of interleaved stereo, `(0, 1)` for mono.
    pub fn measure(samples: &[i16], offset: usize, stride: usize) -> Self {
        let (mut peak, mut sum, mut sum_sq, mut n) = (0u32, 0i64, 0u64, 0u32);
        for &s in samples.iter().skip(offset).step_by(stride.max(1)) {
            peak = peak.max(s.unsigned_abs().into());
            sum += i64::from(s);
            sum_sq += (i64::from(s) * i64::from(s)) as u64;
            n += 1;
        }
        if n == 0 {
            return Self::default();
        }
        let n = n as f32;
        Self {
            peak: peak as f32 / FULL_SCALE,
            rms: libm::sqrtf(sum_sq as f32 / n) / FULL_SCALE,
            dc: sum as f32 / n / FULL_SCALE,
        }
    }

    /// RMS in dBFS (0 dBFS = full scale); `-inf` for digital silence.
    pub fn rms_dbfs(&self) -> f32 {
        to_dbfs(self.rms)
    }

    /// Peak in dBFS.
    pub fn peak_dbfs(&self) -> f32 {
        to_dbfs(self.peak)
    }
}

/// Converts a linear `0.0..=1.0` level to dBFS.
pub fn to_dbfs(linear: f32) -> f32 {
    20.0 * libm::log10f(linear)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_and_full_scale() {
        let l = Level::measure(&[0; 64], 0, 1);
        assert_eq!((l.peak, l.rms, l.dc), (0.0, 0.0, 0.0));
        assert!(l.rms_dbfs().is_infinite(), "silence is -inf dBFS");

        let l = Level::measure(&[i16::MIN; 8], 0, 1);
        assert_eq!(l.peak, 1.0);
        assert!(l.rms_dbfs().abs() < 1e-3, "full scale is 0 dBFS");
    }

    #[test]
    fn stride_selects_one_channel() {
        // Left = 16384 (-6 dBFS), right = 0.
        let lr = [16384, 0, -16384, 0, 16384, 0, -16384, 0];
        let left = Level::measure(&lr, 0, 2);
        let right = Level::measure(&lr, 1, 2);
        assert!((left.rms_dbfs() + 6.02).abs() < 0.01);
        assert_eq!(left.dc, 0.0);
        assert_eq!(right.peak, 0.0);
    }
}
