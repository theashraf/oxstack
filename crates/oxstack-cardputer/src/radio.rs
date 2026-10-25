//! Wi-Fi and Bluetooth LE through Espressif's `esp-radio`.
//!
//! The radio's MAC/PHY firmware is only available from Espressif as binary
//! libraries, which `esp-radio` wraps. This board crate's job is ownership
//! and memory: with the `radio` feature, [`crate::init`] gives the global
//! heap the ~72 KB of RAM the second-stage bootloader used, which is free
//! once the app runs. The radio's buffers therefore don't compete with the
//! framebuffer or audio streams.
//!
//! Enable `wifi` and/or `ble` on this crate for the helpers in
//! [`crate::net`] and [`crate::ble`], or use `esp_radio` directly
//! (re-exported here). Running both at once (coexistence) needs more heap
//! than the reclaimed region provides; add a second region with
//! [`esp_alloc::heap_allocator!`] in the app.

use esp_hal::peripherals::{BT, WIFI};

#[cfg(feature = "radio")]
pub use {esp_alloc, esp_radio};

/// Bytes of reclaimed bootloader RAM handed to the heap (the value
/// `esp-generate` uses for the ESP32-S3).
pub const RECLAIMED_HEAP_BYTES: usize = 73_744;

/// The radio peripherals.
#[allow(missing_docs)]
pub struct RadioResources<'d> {
    pub wifi: WIFI<'d>,
    pub bt: BT<'d>,
}
reborrow_group!(RadioResources { wifi, bt });

#[cfg(feature = "radio")]
pub(crate) fn init_heap() {
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: RECLAIMED_HEAP_BYTES);
}

/// Heap usage in bytes: `(used, free)`. Available with the `radio` feature.
#[cfg(feature = "radio")]
pub fn heap_usage() -> (usize, usize) {
    (esp_alloc::HEAP.used(), esp_alloc::HEAP.free())
}
