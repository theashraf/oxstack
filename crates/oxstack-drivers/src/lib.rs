//! Chip drivers for M5Stack hardware.
//!
//! Every driver here is written against the portable `embedded-hal` /
//! `embedded-hal-async` 1.0 traits, never against a concrete HAL. That keeps
//! them testable on the host (see each module's tests) and reusable on any
//! board that carries the same chip.
//!
//! Design rules shared by all drivers:
//! - no allocation, no global state, no logging in hot paths;
//! - the driver owns its bus handle; sharing a bus is the caller's job
//!   (e.g. `embassy-embedded-hal`'s `I2cDevice`);
//! - register maps are private; the public API speaks in domain types.
#![no_std]
#![deny(unsafe_code)]
#![warn(missing_docs)]

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

pub mod axes;
#[cfg(feature = "bmi270")]
pub mod bmi270;
#[cfg(feature = "es8311")]
pub mod es8311;
#[cfg(feature = "st7789")]
pub mod st7789;
#[cfg(feature = "tca8418")]
pub mod tca8418;

#[cfg(test)]
pub(crate) mod test_util;
