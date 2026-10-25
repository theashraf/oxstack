//! Battery voltage on GPIO10 (ADC1) through a 100 kΩ / 100 kΩ divider.
//!
//! The board routes no charge-status or USB-presence signal to a GPIO, so
//! voltage is the only battery information available. While USB power is
//! connected, the reading is the charger's output, not the cell's resting
//! voltage.

use esp_hal::Async;
use esp_hal::analog::adc::{Adc, AdcCalCurve, AdcConfig, AdcPin, Attenuation};
use esp_hal::peripherals::{ADC1, GPIO10};

/// The divider halves the battery voltage.
const DIVIDER_RATIO: u32 = 2;
/// Samples averaged per reading.
const SAMPLES: u32 = 16;

type Cal<'d> = AdcCalCurve<ADC1<'d>>;

/// ADC1 and the sense pin.
#[allow(missing_docs)]
pub struct BatteryResources<'d> {
    pub adc: ADC1<'d>,
    pub pin: GPIO10<'d>,
}
reborrow_group!(BatteryResources { adc, pin });

impl<'d> BatteryResources<'d> {
    /// Configures ADC1 with eFuse curve calibration on the sense pin.
    pub fn init(&mut self) -> Battery<'_> {
        let mut config = AdcConfig::new();
        // 11 dB covers ~0–3.1 V at the pin, i.e. a 0–6.2 V battery.
        let pin = config.enable_pin_with_cal(self.pin.reborrow(), Attenuation::_11dB);
        let adc = Adc::new(self.adc.reborrow(), config).into_async();
        Battery { adc, pin }
    }
}

/// Battery voltage monitor.
pub struct Battery<'d> {
    adc: Adc<'d, ADC1<'d>, Async>,
    pin: AdcPin<GPIO10<'d>, ADC1<'d>, Cal<'d>>,
}

/// Rough state of charge for a single Li-ion cell from its voltage, in %.
///
/// Piecewise-linear over a typical resting-voltage curve. It is only an
/// estimate: it reads high while charging (the charger holds ~4.2 V) and
/// low under heavy load (Wi-Fi bursts, speaker).
pub const fn estimate_percent(millivolts: u16) -> u8 {
    const CURVE: [(u16, u8); 10] = [
        (3300, 0),
        (3400, 5),
        (3500, 10),
        (3600, 20),
        (3700, 35),
        (3800, 50),
        (3900, 65),
        (4000, 80),
        (4100, 90),
        (4200, 100),
    ];
    if millivolts <= CURVE[0].0 {
        return 0;
    }
    let mut i = 1;
    while i < CURVE.len() {
        let (v1, p1) = CURVE[i];
        if millivolts <= v1 {
            let (v0, p0) = CURVE[i - 1];
            let span = (millivolts - v0) as u32 * (p1 - p0) as u32 / (v1 - v0) as u32;
            return p0 + span as u8;
        }
        i += 1;
    }
    100
}

const _: () = {
    assert!(estimate_percent(3000) == 0);
    assert!(estimate_percent(3300) == 0);
    assert!(estimate_percent(3750) == 42); // halfway between 35 % and 50 %
    assert!(estimate_percent(4200) == 100);
    assert!(estimate_percent(4400) == 100);
};

impl Battery<'_> {
    /// Battery voltage in millivolts, averaged over 16 calibrated samples.
    pub async fn millivolts(&mut self) -> u16 {
        let mut sum = 0u32;
        for _ in 0..SAMPLES {
            // With a calibration scheme, readings are already in mV.
            sum += u32::from(self.adc.read_oneshot(&mut self.pin).await);
        }
        (sum / SAMPLES * DIVIDER_RATIO) as u16
    }

    /// [`estimate_percent`] of a fresh reading.
    pub async fn percent(&mut self) -> u8 {
        estimate_percent(self.millivolts().await)
    }
}
