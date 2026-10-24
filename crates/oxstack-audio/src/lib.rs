//! Audio building blocks that know nothing about hardware.
//!
//! Samples are signed 16-bit PCM, the format every oxstack codec path uses.
//! Multi-channel data is interleaved (`L R L R …`).
#![no_std]
#![deny(unsafe_code)]
#![warn(missing_docs)]

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

pub mod level;
pub mod tone;
