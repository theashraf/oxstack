//! Board support for the **M5Stack Cardputer ADV** (ESP32-S3FN8, Stamp-S3A).
//!
//! ```ignore
//! #[esp_rtos::main]
//! async fn main(_spawner: Spawner) -> ! {
//!     let mut board = oxstack_cardputer::init(oxstack_cardputer::default_config());
//!     let _backlight = board.backlight.init();
//!     let i2c = board.sys_i2c.into_bus();
//!     let mut display = board.display.init().await.unwrap();
//!     let mut keyboard = board.keyboard.init(i2c).await.unwrap();
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
//! [`Board`] splits the chip's peripherals into one *resource group* per
//! board function: `display`, `keyboard`, `audio`, `spi_bus` and so on. Each
//! group owns exactly the pins and peripherals that function is wired to, so
//! the compiler rules out two drivers fighting over a pin. Groups you never
//! `init` cost nothing.
//!
//! Every group has an `init` that **borrows** the group (`&mut self`) and
//! returns a driver tied to that borrow. Drop the driver and the group is
//! free again, so one firmware can start the same driver twice, or the
//! microphone after the speaker, without rebooting:
//!
//! ```ignore
//! let mut mic = board.audio.recorder(i2c, rate, gain).await?;
//! // ... record ...
//! drop(mic);
//! let mut speaker = board.audio.player(i2c, rate, volume).await?; // same hardware
//! ```
//!
//! Groups are plain structs of `esp-hal` peripherals with a `'d` lifetime.
//! Destructure one to drive the pins yourself, or `reborrow()` it to lend a
//! shorter-lived copy to a function, exactly as with `esp-hal` peripherals.
//!
//! Three things are one-per-boot by nature and keep a consuming API: the
//! shared I²C bus ([`i2c::SysI2cResources::into_bus`]), the framebuffer
//! ([`display::framebuffer`]) and the Wi-Fi station ([`net::start_station`]).
//!
//! # Project setup
//!
//! The quickest start is the template in the repository:
//!
//! ```sh
//! cargo install cargo-generate espup espflash --locked
//! espup install && source ~/export-esp.sh
//! cargo generate --git https://github.com/theashraf/oxstack templates/cardputer-app
//! ```
//!
//! By hand, a firmware crate needs:
//!
//! * **Dependencies** at the versions this crate pins. The ones you use in
//!   types are re-exported here ([`esp_hal`], [`esp_rtos`],
//!   [`embassy_executor`], [`embassy_time`], [`embassy_sync`],
//!   [`embassy_futures`], [`esp_bootloader_esp_idf`]). Add `esp-backtrace`
//!   (panic handler) and `esp-println` with `defmt-espflash` + `jtag-serial`
//!   (log transport) yourself; a library must not pull in a panic handler.
//!   `#[esp_rtos::main]` expands to paths in `esp_rtos` and
//!   `embassy_executor`, so list those two as direct dependencies too.
//! * **`rust-toolchain.toml`** selecting the `esp` channel (`espup install`).
//! * **`.cargo/config.toml`**: target `xtensa-esp32s3-none-elf`,
//!   `build-std = ["core", "alloc"]`, `rustflags = ["-C", "link-arg=-nostartfiles"]`
//!   and an `espflash flash --monitor --log-format defmt` runner.
//! * **`build.rs`** emitting `-Tdefmt.x` and, last, `-Tlinkall.x`.
//! * `esp_bootloader_esp_idf::esp_app_desc!();` once in `main.rs`.
//!
//! API docs are built for the ESP32-S3 and hosted at
//! <https://theashraf.github.io/oxstack/oxstack_cardputer/>; docs.rs cannot
//! build for Xtensa.
#![no_std]
#![warn(missing_docs)]

#[cfg(feature = "radio")]
extern crate alloc;

/// Implements `reborrow` for a resource group: a copy of the group whose
/// lifetime is that of the borrow, exactly like `esp-hal` peripherals.
macro_rules! reborrow_group {
    ($ty:ident { $($field:ident),* $(,)? }) => {
        impl<'d> $ty<'d> {
            /// A copy of this group that lives only as long as the borrow,
            /// to lend the hardware to a function without giving it up.
            pub fn reborrow(&mut self) -> $ty<'_> {
                $ty { $($field: self.$field.reborrow()),* }
            }
        }
    };
}

/// `(buffer, descriptors)` for a DMA buffer of `$size` bytes split into
/// `$chunk`-byte descriptors, backed by statics in internal RAM.
///
/// Each expansion owns one pair of statics and hands out `'static`
/// references to them, so the caller must guarantee that references from
/// the previous expansion are dead. Resource groups ensure this by borrowing
/// themselves for the lifetime of the driver that holds the buffer.
macro_rules! static_dma_memory {
    ($size:expr, $chunk:expr) => {{
        use esp_hal::dma::DmaDescriptor;
        use esp_hal::dma::aligned::InternalMemory;
        const COUNT: usize = esp_hal::dma::descriptor_count($size, $chunk);
        static mut BUFFER: InternalMemory<[u8; $size]> = InternalMemory::new([0; $size]);
        static mut DESCRIPTORS: InternalMemory<[DmaDescriptor; COUNT]> =
            InternalMemory::new([DmaDescriptor::EMPTY; COUNT]);
        // SAFETY: see above. `&raw mut` creates no intermediate reference.
        unsafe {
            (
                (*&raw mut BUFFER).get_mut().unsize(),
                (*&raw mut DESCRIPTORS).get_mut().unsize(),
            )
        }
    }};
}

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
pub mod rmt;
pub mod sd;

pub use board::*;
pub use oxstack_audio as dsp;
pub use oxstack_drivers as drivers;
pub use oxstack_gfx as gfx;
pub use oxstack_input as input;
// The crates whose types appear in this API, at the versions it pins.
pub use {
    embassy_executor, embassy_futures, embassy_sync, embassy_time, esp_bootloader_esp_idf, esp_hal,
    esp_rtos,
};

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
