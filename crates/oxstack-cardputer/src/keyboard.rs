//! The 56-key keyboard, scanned by a TCA8418 on the internal I²C bus.
//!
//! The scanner debounces and queues events in hardware and pulls GPIO11
//! low while any are pending. [`Keyboard::next_event`] sleeps on that pin,
//! so an idle keyboard costs no CPU time and no bus traffic.

use esp_hal::gpio::{Input, InputConfig, Pull};
use esp_hal::peripherals::GPIO11;
use oxstack_drivers::tca8418::{self, Event, FIFO_DEPTH, Matrix, Tca8418};
use oxstack_input::{KeyEvent, KeyboardState, cardputer};

use crate::i2c::{SysI2cBus, SysI2cDevice, device};

/// I²C error from the shared bus.
pub type Error = embassy_embedded_hal::shared_bus::I2cDeviceError<esp_hal::i2c::master::Error>;

const MATRIX: Matrix = Matrix::new(cardputer::ADV_MATRIX_ROWS, cardputer::ADV_MATRIX_COLS);

/// The scanner's interrupt line.
#[allow(missing_docs)]
pub struct KeyboardResources {
    pub int: GPIO11<'static>,
}

impl KeyboardResources {
    /// Configures the scanner and returns the keyboard.
    pub async fn init(self, bus: &'static SysI2cBus) -> Result<Keyboard, Error> {
        // The line has a 3.3 kΩ pull-up on the board; the internal one is
        // a harmless backup if the scanner is unpowered.
        let int = Input::new(self.int, InputConfig::default().with_pull(Pull::Up));
        let mut scanner = Tca8418::new(device(bus), tca8418::DEFAULT_ADDRESS);
        scanner.configure(MATRIX).await?;
        Ok(Keyboard {
            scanner,
            int,
            state: KeyboardState::new(),
            fifo: [0; FIFO_DEPTH],
            pos: 0,
            len: 0,
        })
    }
}

/// The keyboard driver.
pub struct Keyboard {
    scanner: Tca8418<SysI2cDevice>,
    int: Input<'static>,
    state: KeyboardState,
    /// Raw events drained from the chip but not yet returned.
    fifo: [u8; FIFO_DEPTH],
    pos: u8,
    len: u8,
}

impl Keyboard {
    /// Waits for the next key press or release.
    ///
    /// Cancelling while it waits for the interrupt line (the usual case,
    /// e.g. in a `select`) loses nothing: events stay queued in the scanner.
    /// Cancelling during the brief I²C drain can drop up to one FIFO's worth
    /// of events.
    pub async fn next_event(&mut self) -> Result<KeyEvent, Error> {
        loop {
            while self.pos < self.len {
                let raw = self.fifo[usize::from(self.pos)];
                self.pos += 1;
                if let Some(Event::Key(k)) = Event::decode(raw)
                    && let Some(key) = cardputer::from_adv_matrix(k.row, k.col)
                {
                    return Ok(self.state.apply(key, k.pressed));
                }
            }
            // Wait on the level, not an edge, so an interrupt that fired
            // before we started waiting is never missed.
            self.int.wait_for_low().await;
            let n = self.scanner.drain(&mut self.fifo).await?.len();
            self.scanner.clear_interrupts().await?;
            (self.pos, self.len) = (0, n as u8);
        }
    }

    /// Held keys and modifiers.
    pub fn state(&self) -> &KeyboardState {
        &self.state
    }
}
