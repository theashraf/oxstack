//! Infrared remote-control protocols, as hardware-independent timings.
//!
//! An encoder produces [`Pulse`]s, each a carrier-on "mark" followed by a
//! carrier-off "space", in microseconds. The board layer turns them into
//! its pulse generator's format (RMT on ESP32) and adds the carrier.
#![no_std]
#![deny(unsafe_code)]
#![warn(missing_docs)]

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

pub mod nec;

/// One carrier burst (`mark_us`) followed by silence (`space_us`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Pulse {
    /// Carrier-on time in microseconds.
    pub mark_us: u16,
    /// Carrier-off time in microseconds (0 for the final mark).
    pub space_us: u16,
}
