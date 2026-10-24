//! BMI270 IMU on the internal I²C bus (0x69).
//!
//! The chip's INT1/INT2 pins are not connected on the Cardputer ADV, so it
//! can only be polled, and it cannot wake the ESP32 from sleep.

use embassy_time::Delay;
use oxstack_drivers::bmi270::{self, ADDRESS_SDO_HIGH, Bmi270};
pub use oxstack_drivers::bmi270::{AccelRange, Config, DataRate, GyroRange, RawSample, Sample};

use crate::i2c::{SysI2cBus, SysI2cDevice, device};

/// The IMU driver on the shared bus.
pub type Imu = Bmi270<SysI2cDevice>;

/// IMU errors.
pub type Error =
    bmi270::Error<embassy_embedded_hal::shared_bus::I2cDeviceError<esp_hal::i2c::master::Error>>;

/// Ownership token for the IMU. It has no pins of its own, but owning it
/// keeps two drivers from initialising the chip.
pub struct ImuResources {
    pub(crate) _private: (),
}

impl ImuResources {
    /// Resets the chip, uploads Bosch's config (~250 ms), and starts sampling.
    pub async fn init(self, bus: &'static SysI2cBus, config: Config) -> Result<Imu, Error> {
        let mut imu = Bmi270::new(device(bus), ADDRESS_SDO_HIGH);
        imu.init(config, &mut Delay).await?;
        Ok(imu)
    }
}
