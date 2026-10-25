//! IR transmitter on GPIO44 (LED driven directly through 22 Ω), using an
//! RMT channel with hardware 38 kHz carrier modulation. Get it from
//! [`crate::rmt::RmtResources::init`].
//!
//! Protocol encoding is in [`oxstack_ir`] (re-exported as [`protocols`]):
//!
//! ```ignore
//! let (_led, mut ir) = board.rmt.init()?;
//! ir.send_nec(0x04, 0x08).await?;
//! ```
//!
//! ⚠️ The carrier timing assumes RMT carrier ticks count the 80 MHz group
//! clock (ESP32-S3 TRM), not the per-channel divided clock. A phone camera
//! shows that the LED fires; only a real IR receiver confirms the 38 kHz.

use esp_hal::Async;
use esp_hal::gpio::Level;
use esp_hal::peripherals::GPIO44;
use esp_hal::rmt::{Channel, ConfigError, Error, PulseCode, Tx, TxChannelConfig, TxChannelCreator};
pub use oxstack_ir as protocols;
use oxstack_ir::{Pulse, nec};

use crate::led::RMT_MHZ;

/// Most pulses one [`IrTx::send`] call accepts (one RMT memory block,
/// minus the end marker).
pub const MAX_PULSES: usize = 47;

/// IR transmit errors.
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum IrError {
    /// More than [`MAX_PULSES`] pulses.
    TooLong,
    /// RMT failure.
    Rmt(Error),
}

/// The IR transmitter.
pub struct IrTx<'d> {
    ch: Channel<'d, Async, Tx>,
}

impl<'d> IrTx<'d> {
    /// Configures an RMT TX channel (1 µs ticks, 38 kHz carrier, idle off).
    /// The `Rmt` must run at [`RMT_MHZ`].
    pub fn new(
        channel: impl TxChannelCreator<'d, Async>,
        pin: GPIO44<'d>,
    ) -> Result<Self, ConfigError> {
        let carrier_ticks = (RMT_MHZ * 1_000_000 / nec::CARRIER_HZ) as u16;
        let config = TxChannelConfig::default()
            .with_clk_divider(RMT_MHZ as u8) // 1 tick = 1 µs
            .with_idle_output(true)
            .with_idle_output_level(Level::Low)
            .with_carrier_modulation(true)
            .with_carrier_level(Level::High)
            .with_carrier_high(carrier_ticks / 2)
            .with_carrier_low(carrier_ticks - carrier_ticks / 2);
        Ok(Self {
            ch: channel.configure_tx(&config)?.with_pin(pin),
        })
    }

    /// Transmits mark/space pulses on the carrier.
    pub async fn send(&mut self, pulses: &[Pulse]) -> Result<(), IrError> {
        if pulses.len() > MAX_PULSES {
            return Err(IrError::TooLong);
        }
        let mut codes = [PulseCode::end_marker(); MAX_PULSES + 1];
        for (code, p) in codes.iter_mut().zip(pulses) {
            // A zero-length space doubles as the end marker after a final mark.
            *code = PulseCode::new(Level::High, p.mark_us, Level::Low, p.space_us);
        }
        self.ch
            .transmit(&codes[..=pulses.len()])
            .await
            .map_err(IrError::Rmt)
    }

    /// Sends one standard NEC frame.
    pub async fn send_nec(&mut self, address: u8, command: u8) -> Result<(), IrError> {
        self.send(&nec::encode(address, command)).await
    }
}
