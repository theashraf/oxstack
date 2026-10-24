//! Board support for the **M5Stack Cardputer ADV** (ESP32-S3FN8, Stamp-S3A).
//!
//! ```ignore
//! #[esp_rtos::main]
//! async fn main(_spawner: Spawner) -> ! {
//!     let board = oxstack_cardputer::init(oxstack_cardputer::default_config());
//!     let i2c = board.sys_i2c.into_bus();
//!     let mut keyboard = board.keyboard.init(i2c).await.unwrap();
//!     let mut display = board.display.init().await.unwrap();
//!     let fb = oxstack_cardputer::display::framebuffer();
//!     loop {
//!         let ev = keyboard.next_event().await.unwrap();
//!         // draw into `fb`, then:
//!         display.flush(fb).await.unwrap();
//!     }
//! }
//! ```
//!
//! # Model
//!
//! [`Board`] splits the chip's peripherals into one resource group per
//! board function, such as `display`, `keyboard` or `sd`. Each group owns
//! exactly the pins and peripherals that function is wired to, so ownership
//! rules out two drivers fighting over a pin. Every group has an `init`
//! that returns a ready driver; groups you never `init` cost nothing, and
//! you can always destructure a group and drive the raw pins yourself.
#![no_std]
#![warn(missing_docs)]

pub mod audio;
pub mod backlight;
pub mod battery;
#[cfg(feature = "ble")]
pub mod ble;
mod board;
pub mod display;
pub mod i2c;
pub mod imu;
pub mod ir;
pub mod keyboard;
pub mod led;
#[cfg(feature = "wifi")]
pub mod net;
pub mod pins;
pub mod radio;
pub mod sd;

pub use board::*;
pub use oxstack_audio as dsp;
pub use oxstack_gfx as gfx;
pub use oxstack_input as input;

/// Initialises the chip, starts the `esp-rtos` scheduler (required for
/// Embassy timers), and returns the board. With the `radio` feature it also
/// adds the reclaimed-RAM heap that `esp-radio` needs.
///
/// Use [`Board::new`] instead to keep control of the timer and software
/// interrupt the scheduler would otherwise claim.
pub fn init(config: esp_hal::Config) -> Board {
    let (board, rtos) = Board::new(esp_hal::init(config));
    #[cfg(feature = "radio")]
    radio::init_heap();
    rtos.start();
    board
}

/// The configuration [`init`] should normally be given: CPU at 240 MHz.
pub fn default_config() -> esp_hal::Config {
    esp_hal::Config::default().with_cpu_clock(esp_hal::clock::CpuClock::max())
}
