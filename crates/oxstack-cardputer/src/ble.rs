//! Bluetooth LE: Espressif's controller (via `esp-radio`) under the
//! pure-Rust `trouble` host stack. Requires the `ble` feature.
//!
//! ```ignore
//! let controller = ble::controller(board.radio.bt)?;
//! let mut resources: HostResources<_, DefaultPacketPool, 1, 2> = HostResources::new();
//! let stack = trouble_host::new(controller, &mut resources)
//!     .set_random_address(ble::address())
//!     .build();
//! ```
//!
//! GATT services are declared in the application with trouble's
//! `#[gatt_server]` / `#[gatt_service]` macros (see `ble_peripheral`).
//!
//! BLE alone fits in the reclaimed-RAM heap. Running Wi-Fi at the same time
//! (coexistence) needs more; see [`crate::radio`].

pub use bt_hci;
use esp_hal::efuse::{self, InterfaceMacAddress};
use esp_hal::peripherals::BT;
use esp_radio::ble::controller::BleConnector;
pub use trouble_host;
use trouble_host::prelude::{Address, ExternalController};

/// HCI command slots the controller adapter can have in flight (the value
/// trouble's ESP32 examples use).
pub const COMMAND_SLOTS: usize = 20;

/// The HCI controller type to hand to `trouble_host::new`.
pub type Controller = ExternalController<BleConnector<'static>, COMMAND_SLOTS>;

/// Error starting the BLE controller.
pub type InitError = esp_radio::ble::controller::BleInitError;

/// Starts the Bluetooth controller.
pub fn controller(bt: BT<'static>) -> Result<Controller, InitError> {
    Ok(ExternalController::new(BleConnector::new(
        bt,
        Default::default(),
    )?))
}

/// This device's BLE address: a static random address derived from the
/// chip's factory Bluetooth MAC, so it is stable across boots and unique per
/// board. (Static random addresses must have the two top bits set.)
pub fn address() -> Address {
    let mac = efuse::interface_mac_address(InterfaceMacAddress::Bluetooth);
    let mut le = [0u8; 6];
    for (dst, src) in le.iter_mut().zip(mac.as_bytes().iter().rev()) {
        *dst = *src; // BLE addresses are little-endian on the air.
    }
    le[5] |= 0xC0;
    Address::random(le)
}
