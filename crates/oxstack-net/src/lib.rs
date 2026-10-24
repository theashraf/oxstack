//! Network protocol pieces that don't depend on any socket implementation.
//!
//! Each module turns bytes into domain values (and back); the caller moves
//! the bytes with whatever stack it has (`embassy-net` on oxstack boards).
//! That keeps the parsing fully testable on the host.
#![no_std]
#![deny(unsafe_code)]
#![warn(missing_docs)]

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

pub mod sntp;
pub mod time;
