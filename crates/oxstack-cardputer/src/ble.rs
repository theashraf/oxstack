//! Bluetooth LE: Espressif's controller (via `esp-radio`) under the
//! pure-Rust `trouble` host stack. Requires the `ble` feature.
//!
//! ```ignore
//! let controller = ble::controller(board.radio.bt.reborrow())?;
//! let mut resources = ble::ScanResources::new();
//! let stack = ble::stack(controller, &mut resources);
//! loop {
//!     ble::scan(&stack, Duration::from_secs(5), |adv| {
//!         info!("{} dBm {} {:?}", adv.rssi, adv.address, adv.name());
//!     })
//!     .await?;
//! }
//! ```
//!
//! GATT services are declared in the application with trouble's
//! `#[gatt_server]` / `#[gatt_service]` macros (see `ble_peripheral`).
//!
//! Create the controller and stack **once per boot**: trouble keeps the
//! host inside the resources without ever dropping it, so a second
//! controller would find the first still registered with the radio.
//! Scans, advertising sessions and connections on that stack can be
//! started as often as needed.
//!
//! BLE alone fits in the reclaimed-RAM heap. Running Wi-Fi at the same time
//! (coexistence) needs more; see [`crate::radio`].

use core::cell::RefCell;

pub use bt_hci;
use bt_hci::param::LeAdvReportsIter;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Timer};
use esp_hal::efuse::{self, InterfaceMacAddress};
use esp_hal::peripherals::BT;
use esp_radio::ble::controller::{BleConnector, BleConnectorError};
pub use trouble_host;
use trouble_host::prelude::{
    AdStructure, Address, DefaultPacketPool, EventHandler, ExternalController, HostResources,
    ScanConfig, Scanner, Stack,
};

/// HCI command slots the controller adapter can have in flight (the value
/// trouble's ESP32 examples use).
pub const COMMAND_SLOTS: usize = 20;

/// The HCI controller type to hand to `trouble_host::new`.
pub type Controller<'d> = ExternalController<BleConnector<'d>, COMMAND_SLOTS>;

/// Error starting the BLE controller.
pub type InitError = esp_radio::ble::controller::BleInitError;

/// Errors from the host stack.
pub type HostError = trouble_host::BleHostError<BleConnectorError>;

/// Host resources sized for scanning (one connection slot, one channel).
/// Peripherals with GATT services usually want more; see the trouble docs.
pub type ScanResources<'d> = HostResources<Controller<'d>, DefaultPacketPool, 1, 1>;

/// Starts the Bluetooth controller.
pub fn controller<'d>(bt: BT<'d>) -> Result<Controller<'d>, InitError> {
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

/// Builds the host stack on `controller` with this device's [`address`].
pub fn stack<
    'r,
    'd,
    const CONNS: usize,
    const CHANNELS: usize,
    const ADV: usize,
    const BONDS: usize,
>(
    controller: Controller<'d>,
    resources: &'r mut HostResources<
        Controller<'d>,
        DefaultPacketPool,
        CONNS,
        CHANNELS,
        ADV,
        BONDS,
    >,
) -> Stack<'r, Controller<'d>, DefaultPacketPool> {
    trouble_host::new(controller, resources)
        .set_random_address(address())
        .build()
}

/// One advertising report, as passed to the [`scan`] callback.
#[derive(Clone, Copy, Debug)]
pub struct Advertisement<'a> {
    /// The advertiser's address.
    pub address: Address,
    /// Signal strength in dBm.
    pub rssi: i8,
    /// Raw advertising (or scan response) data: a sequence of AD structures.
    pub data: &'a [u8],
}

impl<'a> Advertisement<'a> {
    /// The device name, if this report carries one. Names often arrive in
    /// the scan response rather than the advertisement, so remember the
    /// first one seen per address.
    pub fn name(&self) -> Option<&'a str> {
        let mut short = None;
        for ad in AdStructure::decode(self.data).flatten() {
            match ad {
                AdStructure::CompleteLocalName(n) => return core::str::from_utf8(n).ok(),
                AdStructure::ShortenedLocalName(n) => short = core::str::from_utf8(n).ok(),
                _ => {}
            }
        }
        short
    }

    /// The decoded AD structures (flags, service UUIDs, names, …).
    pub fn structures(&self) -> impl Iterator<Item = AdStructure<'a>> {
        AdStructure::decode(self.data).flatten()
    }
}

struct Reports<F>(RefCell<F>);

impl<F: FnMut(&Advertisement<'_>)> EventHandler for Reports<F> {
    fn on_adv_reports(&self, mut reports: LeAdvReportsIter<'_>) {
        let mut f = self.0.borrow_mut();
        while let Some(Ok(r)) = reports.next() {
            f(&Advertisement {
                address: Address::new(r.addr_kind, r.addr),
                rssi: r.rssi,
                data: r.data,
            });
        }
    }
}

/// Scans for `duration`, calling `on_advertisement` for every report
/// (duplicates included, so signal strength stays current). Active
/// scanning, 100 ms interval and window, so names usually arrive too.
///
/// Runs the host while scanning; nothing else can use `stack` meanwhile.
pub async fn scan<'d>(
    stack: &Stack<'_, Controller<'d>, DefaultPacketPool>,
    duration: Duration,
    on_advertisement: impl FnMut(&Advertisement<'_>),
) -> Result<(), HostError> {
    let mut runner = stack.runner();
    let mut scanner = Scanner::new(stack.central());
    let reports = Reports(RefCell::new(on_advertisement));
    let config = ScanConfig {
        active: true,
        interval: Duration::from_millis(100),
        window: Duration::from_millis(100),
        ..Default::default()
    };
    let session = async {
        let session = scanner.scan(&config).await?;
        Timer::after(duration).await;
        drop(session);
        // The session's drop asks the host to disable scanning; give the
        // runner a moment to send that command before it is stopped.
        Timer::after(Duration::from_millis(100)).await;
        Ok(())
    };
    match select(runner.run_with_handler(&reports), session).await {
        Either::First(r) => r,
        Either::Second(r) => r,
    }
}
