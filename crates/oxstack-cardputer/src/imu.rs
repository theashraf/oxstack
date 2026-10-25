//! BMI270 IMU on the internal I²C bus (0x69).
//!
//! The chip's INT1/INT2 pins are not connected on the Cardputer ADV, so it
//! can only be polled, and it cannot wake the ESP32 from sleep.
//!
//! # Axes
//!
//! [`Imu::read`] returns readings in the **board frame**. Hold the Cardputer
//! in landscape, screen facing you, keyboard below:
//!
//! * **+X** points right,
//! * **+Y** points up, toward the top edge (away from the keyboard),
//! * **+Z** points out of the screen, toward you.
//!
//! Lying flat on a table, screen up, acceleration reads about `[0, 0, +1]` g.
//! Standing on its keyboard edge, screen vertical, it reads `[0, +1, 0]`.
//! The gyro follows the right-hand rule about the same axes.
//!
//! The chip is mounted rotated relative to the board; [`BOARD_AXES`] is the
//! mapping applied. **Only its Z axis is confirmed on hardware so far.** The
//! `imu_axes` example in the repository measures all three in under a
//! minute and prints the constant to use; until then, [`Imu::set_axes`]
//! overrides the default and [`Imu::read_chip`] gives untranslated readings.

use core::marker::PhantomData;

use embassy_time::Delay;
pub use oxstack_drivers::axes::{Axis, AxisMap};
use oxstack_drivers::bmi270::{self, ADDRESS_SDO_HIGH, Bmi270};
pub use oxstack_drivers::bmi270::{AccelRange, Config, DataRate, GyroRange, RawSample, Sample};

use crate::i2c::{SysI2cBus, SysI2cDevice, device};

/// The raw BMI270 driver on the shared bus.
pub type Chip = Bmi270<SysI2cDevice>;

/// IMU errors.
pub type Error =
    bmi270::Error<embassy_embedded_hal::shared_bus::I2cDeviceError<esp_hal::i2c::master::Error>>;

/// How the BMI270's axes map onto the board frame (see the module docs).
///
/// Z is verified: the chip's +Z points out of the screen. X and Y are the
/// chip's own until measured; run the `imu_axes` example to check them.
pub const BOARD_AXES: AxisMap = AxisMap {
    x: Axis::PosX,
    y: Axis::PosY,
    z: Axis::PosZ,
};

/// Ownership token for the IMU. It has no pins of its own, but borrowing it
/// keeps two drivers from initialising the chip at once.
pub struct ImuResources {
    pub(crate) _private: (),
}

impl ImuResources {
    /// Resets the chip, uploads Bosch's config (~250 ms), and starts sampling
    /// with readings in the board frame ([`BOARD_AXES`]).
    pub async fn init(
        &mut self,
        bus: &'static SysI2cBus,
        config: Config,
    ) -> Result<Imu<'_>, Error> {
        let mut chip = Bmi270::new(device(bus), ADDRESS_SDO_HIGH);
        chip.init(config, &mut Delay).await?;
        Ok(Imu {
            chip,
            axes: BOARD_AXES,
            _token: PhantomData,
        })
    }
}

/// The IMU driver.
pub struct Imu<'d> {
    chip: Chip,
    axes: AxisMap,
    _token: PhantomData<&'d mut ()>,
}

impl Imu<'_> {
    /// One sample in the board frame: acceleration in g, rate in °/s.
    pub async fn read(&mut self) -> Result<Sample, Error> {
        let s = self.read_chip().await?;
        Ok(Sample {
            accel: self.axes.apply(s.accel),
            gyro: self.axes.apply(s.gyro),
        })
    }

    /// One sample in the chip's own frame.
    pub async fn read_chip(&mut self) -> Result<Sample, Error> {
        self.chip.read().await.map_err(Error::I2c)
    }

    /// One unscaled sample in the chip's own frame.
    pub async fn read_raw(&mut self) -> Result<RawSample, Error> {
        self.chip.read_raw().await.map_err(Error::I2c)
    }

    /// Die temperature in °C, or `None` if the sensor reports it invalid.
    pub async fn temperature(&mut self) -> Result<Option<f32>, Error> {
        self.chip.temperature().await.map_err(Error::I2c)
    }

    /// Changes ranges and data rate.
    pub async fn configure(&mut self, config: Config) -> Result<(), Error> {
        self.chip.configure(config).await
    }

    /// The chip-to-board axis mapping [`read`](Self::read) applies.
    pub fn axes(&self) -> AxisMap {
        self.axes
    }

    /// Replaces the axis mapping, e.g. with one measured by `imu_axes`.
    pub fn set_axes(&mut self, axes: AxisMap) {
        self.axes = axes;
    }

    /// The underlying driver, for advanced control.
    pub fn chip(&mut self) -> &mut Chip {
        &mut self.chip
    }
}
