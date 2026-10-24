//! Bosch BMI270 6-axis IMU (accelerometer + gyroscope) over I²C.
//!
//! The BMI270 boots without firmware for its feature engine; until the host
//! uploads Bosch's 8 KB configuration blob it doesn't produce data. The
//! blob ([`CONFIG_FILE`]) is linked into flash once and streamed to the chip
//! in [`UPLOAD_CHUNK`]-byte pieces, so a shared bus is never held for long.
//!
//! Only polling is supported; the interrupt pins are not used.

use embedded_hal_async::delay::DelayNs;
use embedded_hal_async::i2c::{I2c, Operation};

/// I²C address with SDO pulled high.
pub const ADDRESS_SDO_HIGH: u8 = 0x69;
/// I²C address with SDO pulled low.
pub const ADDRESS_SDO_LOW: u8 = 0x68;
/// Value of the `CHIP_ID` register.
pub const CHIP_ID: u8 = 0x24;

/// Bosch's configuration file for the BMI270 feature engine, from
/// `BMI270_SensorAPI` commit `41129fcfe39c` (BSD-3-Clause, © Bosch
/// Sensortec; see `LICENSE-BOSCH` next to this file).
pub static CONFIG_FILE: &[u8; 8192] = include_bytes!("config.bin");

/// Bytes written per I²C transaction during the config upload. Must be even
/// (the chip addresses its init memory in 16-bit words).
pub const UPLOAD_CHUNK: usize = 256;
const _: () =
    assert!(UPLOAD_CHUNK.is_multiple_of(2) && CONFIG_FILE.len().is_multiple_of(UPLOAD_CHUNK));

mod reg {
    pub const CHIP_ID: u8 = 0x00;
    pub const ACC_X_LSB: u8 = 0x0C;
    pub const INTERNAL_STATUS: u8 = 0x21;
    pub const TEMPERATURE_0: u8 = 0x22;
    pub const ACC_CONF: u8 = 0x40;
    pub const ACC_RANGE: u8 = 0x41;
    pub const GYR_CONF: u8 = 0x42;
    pub const GYR_RANGE: u8 = 0x43;
    pub const INIT_CTRL: u8 = 0x59;
    pub const INIT_ADDR_0: u8 = 0x5B;
    pub const INIT_DATA: u8 = 0x5E;
    pub const PWR_CONF: u8 = 0x7C;
    pub const PWR_CTRL: u8 = 0x7D;
    pub const CMD: u8 = 0x7E;
}

const CMD_SOFT_RESET: u8 = 0xB6;
/// `PWR_CTRL`: gyroscope, accelerometer and temperature sensor on.
const PWR_CTRL_GYR_ACC_TEMP: u8 = 0x0E;
/// `PWR_CONF`: advanced power save off, FIFO self-wakeup on (Bosch default
/// for performance mode).
const PWR_CONF_PERFORMANCE: u8 = 0x02;
/// `INTERNAL_STATUS.message` value meaning the config upload was accepted.
const STATUS_INIT_OK: u8 = 0x01;

/// Accelerometer full-scale range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum AccelRange {
    /// ±2 g
    G2,
    /// ±4 g
    G4,
    /// ±8 g
    G8,
    /// ±16 g
    G16,
}

impl AccelRange {
    const fn lsb_per_g(self) -> f32 {
        match self {
            AccelRange::G2 => 16384.0,
            AccelRange::G4 => 8192.0,
            AccelRange::G8 => 4096.0,
            AccelRange::G16 => 2048.0,
        }
    }
}

/// Gyroscope full-scale range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum GyroRange {
    /// ±2000 °/s
    Dps2000,
    /// ±1000 °/s
    Dps1000,
    /// ±500 °/s
    Dps500,
    /// ±250 °/s
    Dps250,
    /// ±125 °/s
    Dps125,
}

impl GyroRange {
    const fn lsb_per_dps(self) -> f32 {
        match self {
            GyroRange::Dps2000 => 16.384,
            GyroRange::Dps1000 => 32.768,
            GyroRange::Dps500 => 65.536,
            GyroRange::Dps250 => 131.072,
            GyroRange::Dps125 => 262.144,
        }
    }
}

/// Output data rate shared by both sensors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[allow(missing_docs)]
pub enum DataRate {
    Hz25 = 0x06,
    Hz50 = 0x07,
    Hz100 = 0x08,
    Hz200 = 0x09,
    Hz400 = 0x0A,
    Hz800 = 0x0B,
    Hz1600 = 0x0C,
}

/// Sensor configuration applied by [`Bmi270::init`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Config {
    /// Accelerometer range.
    pub accel_range: AccelRange,
    /// Gyroscope range.
    pub gyro_range: GyroRange,
    /// Sampling rate of both sensors.
    pub rate: DataRate,
}

impl Default for Config {
    /// ±8 g, ±2000 °/s, 100 Hz: the chip's own reset defaults.
    fn default() -> Self {
        Self {
            accel_range: AccelRange::G8,
            gyro_range: GyroRange::Dps2000,
            rate: DataRate::Hz100,
        }
    }
}

impl Config {
    /// `ACC_CONF`: performance filter, normal bandwidth, rate.
    const fn acc_conf(&self) -> u8 {
        0x80 | (0x02 << 4) | self.rate as u8
    }

    /// `GYR_CONF`: performance filter, normal bandwidth, rate.
    const fn gyr_conf(&self) -> u8 {
        0x80 | (0x02 << 4) | self.rate as u8
    }
}

/// Raw two's-complement readings, X/Y/Z.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct RawSample {
    /// Accelerometer.
    pub accel: [i16; 3],
    /// Gyroscope.
    pub gyro: [i16; 3],
}

/// Scaled readings, X/Y/Z.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Sample {
    /// Acceleration in g.
    pub accel: [f32; 3],
    /// Angular rate in degrees per second.
    pub gyro: [f32; 3],
}

/// Driver errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error<E> {
    /// Bus error.
    I2c(E),
    /// `CHIP_ID` was not [`CHIP_ID`].
    WrongChip(u8),
    /// The chip did not accept the config file; holds `INTERNAL_STATUS`.
    InitFailed(u8),
}

impl<E> From<E> for Error<E> {
    fn from(e: E) -> Self {
        Error::I2c(e)
    }
}

/// BMI270 driver.
pub struct Bmi270<I2C> {
    i2c: I2C,
    addr: u8,
    config: Config,
}

impl<I2C: I2c> Bmi270<I2C> {
    /// Wraps a bus handle. Call [`init`](Self::init) before reading.
    pub const fn new(i2c: I2C, addr: u8) -> Self {
        Self {
            i2c,
            addr,
            config: Config {
                accel_range: AccelRange::G8,
                gyro_range: GyroRange::Dps2000,
                rate: DataRate::Hz100,
            },
        }
    }

    /// Reads `CHIP_ID`.
    pub async fn chip_id(&mut self) -> Result<u8, I2C::Error> {
        self.read_u8(reg::CHIP_ID).await
    }

    /// Resets the chip, uploads [`CONFIG_FILE`], and starts both sensors
    /// with `config`. Takes roughly 250 ms at 400 kHz, mostly the upload.
    pub async fn init(
        &mut self,
        config: Config,
        delay: &mut impl DelayNs,
    ) -> Result<(), Error<I2C::Error>> {
        let id = self.chip_id().await?;
        if id != CHIP_ID {
            return Err(Error::WrongChip(id));
        }

        self.write(reg::CMD, CMD_SOFT_RESET).await?;
        delay.delay_ms(2).await;
        // Advanced power save must be off while loading (datasheet: 450 µs).
        self.write(reg::PWR_CONF, 0x00).await?;
        delay.delay_us(450).await;

        self.write(reg::INIT_CTRL, 0x00).await?;
        for (i, chunk) in CONFIG_FILE.chunks(UPLOAD_CHUNK).enumerate() {
            let word = (i * UPLOAD_CHUNK / 2) as u16;
            // INIT_ADDR_0 holds word bits 0..3, INIT_ADDR_1 bits 4..11.
            let addr = [reg::INIT_ADDR_0, (word & 0x0F) as u8, (word >> 4) as u8];
            self.i2c.write(self.addr, &addr).await?;
            // Register byte and payload as one burst, without copying the
            // chunk out of flash.
            self.i2c
                .transaction(
                    self.addr,
                    &mut [Operation::Write(&[reg::INIT_DATA]), Operation::Write(chunk)],
                )
                .await?;
        }
        self.write(reg::INIT_CTRL, 0x01).await?;

        // The datasheet allows up to 20 ms for initialisation.
        let mut status = 0;
        for _ in 0..25 {
            delay.delay_ms(1).await;
            status = self.read_u8(reg::INTERNAL_STATUS).await? & 0x0F;
            if status == STATUS_INIT_OK {
                break;
            }
        }
        if status != STATUS_INIT_OK {
            return Err(Error::InitFailed(status));
        }

        self.configure(config).await
    }

    /// Changes ranges and rate. The chip must already be initialised.
    pub async fn configure(&mut self, config: Config) -> Result<(), Error<I2C::Error>> {
        self.write(reg::ACC_CONF, config.acc_conf()).await?;
        self.write(reg::ACC_RANGE, config.accel_range as u8).await?;
        self.write(reg::GYR_CONF, config.gyr_conf()).await?;
        self.write(reg::GYR_RANGE, config.gyro_range as u8).await?;
        self.write(reg::PWR_CTRL, PWR_CTRL_GYR_ACC_TEMP).await?;
        self.write(reg::PWR_CONF, PWR_CONF_PERFORMANCE).await?;
        self.config = config;
        Ok(())
    }

    /// Reads both sensors in one 12-byte burst.
    pub async fn read_raw(&mut self) -> Result<RawSample, I2C::Error> {
        let mut b = [0u8; 12];
        self.i2c
            .write_read(self.addr, &[reg::ACC_X_LSB], &mut b)
            .await?;
        let v = |i: usize| i16::from_le_bytes([b[i], b[i + 1]]);
        Ok(RawSample {
            accel: [v(0), v(2), v(4)],
            gyro: [v(6), v(8), v(10)],
        })
    }

    /// Reads both sensors, scaled to g and °/s.
    pub async fn read(&mut self) -> Result<Sample, I2C::Error> {
        let raw = self.read_raw().await?;
        Ok(self.scale(raw))
    }

    /// Converts a raw sample using the current ranges.
    pub fn scale(&self, raw: RawSample) -> Sample {
        let (a, g) = (
            self.config.accel_range.lsb_per_g(),
            self.config.gyro_range.lsb_per_dps(),
        );
        Sample {
            accel: raw.accel.map(|v| f32::from(v) / a),
            gyro: raw.gyro.map(|v| f32::from(v) / g),
        }
    }

    /// Die temperature in °C, or `None` while no valid reading exists yet.
    pub async fn temperature(&mut self) -> Result<Option<f32>, I2C::Error> {
        let mut b = [0u8; 2];
        self.i2c
            .write_read(self.addr, &[reg::TEMPERATURE_0], &mut b)
            .await?;
        let raw = i16::from_le_bytes(b);
        // 0x8000 marks "invalid"; otherwise 1/512 K per LSB around 23 °C.
        Ok((raw != i16::MIN).then(|| 23.0 + f32::from(raw) / 512.0))
    }

    /// Returns the bus handle.
    pub fn release(self) -> I2C {
        self.i2c
    }

    async fn write(&mut self, reg: u8, value: u8) -> Result<(), I2C::Error> {
        self.i2c.write(self.addr, &[reg, value]).await
    }

    async fn read_u8(&mut self, reg: u8) -> Result<u8, I2C::Error> {
        let mut b = [0];
        self.i2c.write_read(self.addr, &[reg], &mut b).await?;
        Ok(b[0])
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::test_util::block_on;
    use core::convert::Infallible;
    use std::vec::Vec;

    /// Register-level model: tracks the init-memory upload and reports
    /// INIT_OK only if the full blob arrived in order.
    struct FakeImu {
        regs: [u8; 128],
        init_mem: Vec<u8>,
        sample: [u8; 12],
    }

    impl FakeImu {
        fn new() -> Self {
            let mut regs = [0; 128];
            regs[usize::from(reg::CHIP_ID)] = CHIP_ID;
            Self {
                regs,
                init_mem: Vec::new(),
                sample: [0; 12],
            }
        }

        fn write(&mut self, bytes: &[u8]) {
            let (start, data) = (bytes[0], &bytes[1..]);
            if start == reg::INIT_DATA {
                let word = usize::from(self.regs[0x5B] & 0x0F) | usize::from(self.regs[0x5C]) << 4;
                assert_eq!(
                    word * 2,
                    self.init_mem.len(),
                    "chunk written at the wrong offset"
                );
                self.init_mem.extend_from_slice(data);
                return;
            }
            for (i, v) in data.iter().enumerate() {
                self.regs[usize::from(start) + i] = *v;
            }
            if start == reg::INIT_CTRL && data == [0x01] && self.init_mem.as_slice() == CONFIG_FILE
            {
                self.regs[usize::from(reg::INTERNAL_STATUS)] = STATUS_INIT_OK;
            }
        }
    }

    impl embedded_hal_async::i2c::ErrorType for FakeImu {
        type Error = Infallible;
    }

    impl I2c for FakeImu {
        async fn transaction(
            &mut self,
            addr: u8,
            ops: &mut [Operation<'_>],
        ) -> Result<(), Infallible> {
            assert_eq!(addr, ADDRESS_SDO_HIGH);
            // Adjacent writes form one burst, as on a real bus.
            let mut burst = Vec::new();
            let mut ptr = 0;
            for op in ops.iter_mut() {
                match op {
                    Operation::Write(b) => burst.extend_from_slice(b),
                    Operation::Read(out) => {
                        if !burst.is_empty() {
                            ptr = burst[0];
                            burst.clear();
                        }
                        for (i, o) in out.iter_mut().enumerate() {
                            let r = usize::from(ptr) + i;
                            *o = if (0x0C..0x18).contains(&r) {
                                self.sample[r - 0x0C]
                            } else {
                                self.regs[r]
                            };
                        }
                    }
                }
            }
            if burst.len() > 1 {
                self.write(&burst);
            }
            Ok(())
        }
    }

    struct NoDelay;
    impl DelayNs for NoDelay {
        async fn delay_ns(&mut self, _ns: u32) {}
    }

    #[test]
    fn init_uploads_whole_config_and_configures() {
        let mut imu = Bmi270::new(FakeImu::new(), ADDRESS_SDO_HIGH);
        let cfg = Config {
            accel_range: AccelRange::G4,
            gyro_range: GyroRange::Dps500,
            rate: DataRate::Hz200,
        };
        block_on(imu.init(cfg, &mut NoDelay)).unwrap();
        let chip = imu.release();
        assert_eq!(chip.init_mem.len(), 8192);
        assert_eq!(chip.regs[usize::from(reg::ACC_CONF)], 0xA9);
        assert_eq!(chip.regs[usize::from(reg::ACC_RANGE)], 0x01);
        assert_eq!(chip.regs[usize::from(reg::GYR_RANGE)], 0x02);
        assert_eq!(chip.regs[usize::from(reg::PWR_CTRL)], 0x0E);
    }

    #[test]
    fn init_rejects_other_chips() {
        let mut fake = FakeImu::new();
        fake.regs[0] = 0x26;
        let mut imu = Bmi270::new(fake, ADDRESS_SDO_HIGH);
        assert_eq!(
            block_on(imu.init(Config::default(), &mut NoDelay)),
            Err(Error::WrongChip(0x26))
        );
    }

    #[test]
    fn samples_are_little_endian_and_scaled() {
        let mut fake = FakeImu::new();
        // accel z = +4096 (1 g at ±8 g); gyro x = -164 (≈ -10 °/s at ±2000).
        fake.sample[4..6].copy_from_slice(&4096i16.to_le_bytes());
        fake.sample[6..8].copy_from_slice(&(-164i16).to_le_bytes());
        let mut imu = Bmi270::new(fake, ADDRESS_SDO_HIGH);
        let s = block_on(imu.read()).unwrap();
        assert_eq!(s.accel, [0.0, 0.0, 1.0]);
        assert!((s.gyro[0] + 10.0).abs() < 0.01);
    }
}
