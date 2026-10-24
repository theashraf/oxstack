//! Keyboard input model.
//!
//! Hardware-agnostic layers on top of a key-matrix scanner:
//!
//! 1. a board-specific matrix map turns a scanner position into a physical
//!    [`Key`] (see [`cardputer`]);
//! 2. [`KeyboardState`] tracks which keys are held and derives
//!    [`Modifiers`], producing a [`KeyEvent`] for every change;
//! 3. [`KeyEvent::action`] resolves the printed legend and layer (Shift, Fn)
//!    into an [`Action`] such as a character or an arrow key.
//!
//! Everything is `const`-table or `match` based, so lookups compile to
//! jump tables in flash, and [`KeyboardState`] is a single `u64` bit set
//! plus one byte of modifiers.
#![no_std]
#![warn(missing_docs)]

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

pub mod cardputer;
mod key;
mod state;

pub use key::{Action, Key, Modifiers};
pub use state::{KeyEvent, KeyboardState};
