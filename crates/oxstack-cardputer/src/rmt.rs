//! The RMT pulse generator and the two LEDs it drives: the WS2812-class RGB
//! LED (GPIO21) and the IR transmitter (GPIO44).
//!
//! Both need an RMT channel from one `Rmt` instance at one clock, so a
//! single [`RmtResources::init`] sets up the peripheral and returns both
//! drivers:
//!
//! ```ignore
//! let (mut led, mut ir) = board.rmt.init()?;
//! led.set(0, 32, 0).await?;
//! ir.send_nec(0x04, 0x08).await?;
//! ```
//!
//! Channels 2–3 stay free; build your own from the group's fields if needed
//! (see [`crate::led::RgbLed::new`] for the pattern).

use esp_hal::peripherals::{GPIO21, GPIO44, RMT};
use esp_hal::rmt::{ConfigError, Rmt};
use esp_hal::time::Rate;

use crate::ir::IrTx;
use crate::led::{RMT_MHZ, RgbLed};

/// The RMT clock both drivers' timings assume.
pub const CLOCK: Rate = Rate::from_mhz(RMT_MHZ);

/// The RMT peripheral and the pins of the LEDs it drives.
#[allow(missing_docs)]
pub struct RmtResources<'d> {
    pub rmt: RMT<'d>,
    pub rgb_led: GPIO21<'d>,
    pub ir_tx: GPIO44<'d>,
}
reborrow_group!(RmtResources {
    rmt,
    rgb_led,
    ir_tx
});

impl<'d> RmtResources<'d> {
    /// Starts the RMT at [`CLOCK`] and configures channel 0 for the RGB LED
    /// and channel 1 for the IR LED.
    pub fn init(&mut self) -> Result<(RgbLed<'_>, IrTx<'_>), ConfigError> {
        let rmt = Rmt::new(self.rmt.reborrow(), CLOCK)?.into_async();
        let led = RgbLed::new(rmt.channel0, self.rgb_led.reborrow())?;
        let ir = IrTx::new(rmt.channel1, self.ir_tx.reborrow())?;
        Ok((led, ir))
    }
}
