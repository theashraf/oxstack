//! The internal I²C bus (GPIO8/9), shared by the TCA8418 keyboard scanner,
//! ES8311 codec and BMI270 IMU.
//!
//! The bus lives in a `static` behind an async mutex, created once per boot
//! by [`SysI2cResources::into_bus`]. Each driver gets its own
//! [`SysI2cDevice`] handle and locks the bus only for the length of one
//! transaction, so tasks on the same executor can use different chips
//! concurrently. Code that didn't create the bus can find it with [`bus`].

use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::once_lock::OnceLock;
use esp_hal::Async;
use esp_hal::i2c::master::{Config, I2c};
use esp_hal::peripherals::{GPIO8, GPIO9, I2C0};
use esp_hal::time::Rate;

/// The shared bus.
pub type SysI2cBus = Mutex<CriticalSectionRawMutex, I2c<'static, Async>>;

/// A handle to one device on the shared bus; implements
/// `embedded_hal_async::i2c::I2c`.
pub type SysI2cDevice = I2cDevice<'static, CriticalSectionRawMutex, I2c<'static, Async>>;

/// Bus clock. All three on-board chips support Fast-mode (400 kHz).
pub const FREQUENCY: Rate = Rate::from_khz(400);

/// I²C0 on GPIO8 (SDA) / GPIO9 (SCL).
#[allow(missing_docs)]
pub struct SysI2cResources<'d> {
    pub i2c: I2C0<'d>,
    pub sda: GPIO8<'d>,
    pub scl: GPIO9<'d>,
}
reborrow_group!(SysI2cResources { i2c, sda, scl });

/// `I2c<Async>` is deliberately `!Send` in esp-hal: its interrupt handler is
/// bound to the core that created it.
struct SharedBus(SysI2cBus);

// SAFETY: the bus is only ever reached through the async mutex inside, and
// oxstack runs its executor on the core that created the bus, so no other
// core touches the driver.
unsafe impl Sync for SharedBus {}

static BUS: OnceLock<SharedBus> = OnceLock::new();

impl SysI2cResources<'static> {
    /// Creates the shared bus. Call it once at start-up; get per-chip handles
    /// with [`device`] and the bus itself, from anywhere, with [`bus`].
    ///
    /// # Panics
    /// If the bus was already created; the resources are unique, so this
    /// only happens through `unsafe` peripheral stealing.
    pub fn into_bus(self) -> &'static SysI2cBus {
        let i2c = I2c::new(self.i2c, Config::default().with_frequency(FREQUENCY))
            .expect("400 kHz is a valid I2C configuration")
            .with_sda(self.sda)
            .with_scl(self.scl)
            .into_async();
        if BUS.init(SharedBus(Mutex::new(i2c))).is_err() {
            panic!("the system I2C bus was already created");
        }
        bus().expect("just initialised")
    }
}

/// The shared bus, if [`SysI2cResources::into_bus`] has been called.
pub fn bus() -> Option<&'static SysI2cBus> {
    BUS.try_get().map(|b| &b.0)
}

/// A new device handle on the shared bus.
pub fn device(bus: &'static SysI2cBus) -> SysI2cDevice {
    I2cDevice::new(bus)
}
