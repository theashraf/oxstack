//! The internal I²C bus (GPIO8/9), shared by the TCA8418 keyboard scanner,
//! ES8311 codec and BMI270 IMU.
//!
//! The bus lives in a `static` behind an async mutex. Each driver gets its
//! own [`SysI2cDevice`] handle and locks the bus only for the length of one
//! transaction, so tasks on the same executor can use different chips
//! concurrently.

use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use esp_hal::Async;
use esp_hal::i2c::master::{Config, I2c};
use esp_hal::peripherals::{GPIO8, GPIO9, I2C0};
use esp_hal::time::Rate;
use static_cell::StaticCell;

/// The shared bus.
pub type SysI2cBus = Mutex<CriticalSectionRawMutex, I2c<'static, Async>>;

/// A handle to one device on the shared bus; implements
/// `embedded_hal_async::i2c::I2c`.
pub type SysI2cDevice = I2cDevice<'static, CriticalSectionRawMutex, I2c<'static, Async>>;

/// Bus clock. All three on-board chips support Fast-mode (400 kHz).
pub const FREQUENCY: Rate = Rate::from_khz(400);

/// I²C0 on GPIO8 (SDA) / GPIO9 (SCL).
#[allow(missing_docs)]
pub struct SysI2cResources {
    pub i2c: I2C0<'static>,
    pub sda: GPIO8<'static>,
    pub scl: GPIO9<'static>,
}

impl SysI2cResources {
    /// Creates the shared bus. Get per-chip handles with [`device`].
    ///
    /// # Panics
    /// If the bus was already created; the resources are unique, so this
    /// only happens through `unsafe` peripheral stealing.
    pub fn into_bus(self) -> &'static SysI2cBus {
        static BUS: StaticCell<SysI2cBus> = StaticCell::new();
        let i2c = I2c::new(self.i2c, Config::default().with_frequency(FREQUENCY))
            .expect("400 kHz is a valid I2C configuration")
            .with_sda(self.sda)
            .with_scl(self.scl)
            .into_async();
        BUS.init(Mutex::new(i2c))
    }
}

/// A new device handle on the shared bus.
pub fn device(bus: &'static SysI2cBus) -> SysI2cDevice {
    I2cDevice::new(bus)
}
