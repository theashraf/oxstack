//! Wi-Fi station networking: connect, DHCP, DNS, UDP/TCP via `embassy-net`,
//! plus network time. Requires the `wifi` feature.
//!
//! ```ignore
//! let stack = net::start_station(spawner, board.radio.wifi, Credentials { ssid, password })?;
//! stack.wait_config_up().await;               // associated + DHCP lease
//! let clock = net::sync_clock(stack, net::DEFAULT_NTP_SERVER).await?;
//! let now = clock.now(utc_offset_minutes);    // calendar date/time
//! ```
//!
//! [`start_station`] spawns two tasks: one keeps the link up (reconnecting
//! every 5 s after a drop), the other runs the IP stack. Link state and
//! signal strength are published in atomics ([`link`]), so any task can
//! show them without owning the radio.

use core::sync::atomic::{AtomicI8, AtomicU8, Ordering};

use embassy_executor::{SpawnError, Spawner};
use embassy_futures::select::{Either, select};
use embassy_net::dns::DnsQueryType;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{Runner, Stack, StackResources};
use embassy_time::{Duration, Instant, Timer, with_timeout};
use esp_hal::peripherals::WIFI;
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{
    AuthenticationMethodConfig, Config, ControllerConfig, Interface, WifiController, WifiError,
};
use oxstack_net::sntp;
pub use oxstack_net::sntp::ServerTime;
pub use oxstack_net::time::DateTime;
use static_cell::StaticCell;

pub use embassy_net;

/// A public NTP pool; any SNTP server name or IPv4 literal works.
pub const DEFAULT_NTP_SERVER: &str = "pool.ntp.org";
/// Sockets available to the application (DHCP and DNS use their own).
pub const SOCKETS: usize = 4;

/// Network credentials. An empty password selects an open network.
#[derive(Clone, Copy, Debug)]
pub struct Credentials<'a> {
    /// Network name.
    pub ssid: &'a str,
    /// WPA2/WPA3-Personal passphrase, or empty for an open network.
    pub password: &'a str,
}

/// Errors from [`start_station`].
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// SSID longer than 32 bytes or passphrase longer than 64.
    InvalidCredentials,
    /// The radio driver failed.
    Wifi(WifiError),
    /// The executor had no room for the network tasks.
    Spawn,
}

impl From<SpawnError> for Error {
    fn from(_: SpawnError) -> Self {
        Error::Spawn
    }
}

/// Wi-Fi link status, readable from anywhere.
pub mod link {
    use super::*;

    /// Connection state.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    #[cfg_attr(feature = "defmt", derive(defmt::Format))]
    pub enum State {
        /// Not started yet.
        Idle = 0,
        /// Associating with the access point.
        Connecting = 1,
        /// Associated (an IP address may still be pending; see
        /// `Stack::config_v4`).
        Connected = 2,
        /// The last attempt failed; retrying.
        Retrying = 3,
    }

    pub(super) static STATE: AtomicU8 = AtomicU8::new(State::Idle as u8);
    pub(super) static RSSI: AtomicI8 = AtomicI8::new(i8::MIN);

    /// Current connection state.
    pub fn state() -> State {
        match STATE.load(Ordering::Relaxed) {
            1 => State::Connecting,
            2 => State::Connected,
            3 => State::Retrying,
            _ => State::Idle,
        }
    }

    /// Signal strength in dBm while connected, refreshed every 2 s.
    pub fn rssi() -> Option<i8> {
        let r = RSSI.load(Ordering::Relaxed);
        (state() == State::Connected && r != i8::MIN).then_some(r)
    }

    pub(super) fn set(s: State) {
        STATE.store(s as u8, Ordering::Relaxed);
    }
}

/// Starts Wi-Fi in station mode with DHCP and returns the IP stack. Await
/// `stack.wait_config_up()` before opening sockets.
pub fn start_station(
    spawner: Spawner,
    wifi: WIFI<'static>,
    creds: Credentials<'_>,
) -> Result<Stack<'static>, Error> {
    let ssid = creds
        .ssid
        .try_into()
        .map_err(|_| Error::InvalidCredentials)?;
    let auth = if creds.password.is_empty() {
        AuthenticationMethodConfig::Open
    } else {
        // Acts as the minimum: WPA3-capable networks are still accepted.
        AuthenticationMethodConfig::Wpa2Personal(
            creds
                .password
                .try_into()
                .map_err(|_| Error::InvalidCredentials)?,
        )
    };
    let station = Config::Station(
        StationConfig::default()
            .with_ssid(ssid)
            .with_authentication(auth),
    );

    let interface = Interface::station();
    let controller = WifiController::new(
        wifi,
        ControllerConfig::default().with_initial_config(station),
    )
    .map_err(Error::Wifi)?;

    static RESOURCES: StaticCell<StackResources<{ SOCKETS + 2 }>> = StaticCell::new();
    let rng = esp_hal::rng::Rng::new();
    // The hardware RNG draws entropy from the radio, which is now running.
    let seed = u64::from(rng.random()) << 32 | u64::from(rng.random());
    let (stack, runner) = embassy_net::new(
        interface,
        embassy_net::Config::dhcpv4(Default::default()),
        RESOURCES.init(StackResources::new()),
        seed,
    );

    spawner.spawn(link_task(controller)?);
    spawner.spawn(net_task(runner)?);
    Ok(stack)
}

#[embassy_executor::task]
async fn link_task(mut controller: WifiController<'static>) {
    loop {
        link::set(link::State::Connecting);
        if controller.connect_async().await.is_ok() {
            link::set(link::State::Connected);
            // Refresh RSSI until the link drops.
            loop {
                let rssi = controller
                    .rssi()
                    .ok()
                    .map_or(i8::MIN, |r| r.clamp(-127, 0) as i8);
                link::RSSI.store(rssi, Ordering::Relaxed);
                let dropped = select(
                    controller.wait_for_disconnect_async(),
                    Timer::after(Duration::from_secs(2)),
                )
                .await;
                if let Either::First(_) = dropped {
                    break;
                }
            }
        }
        link::set(link::State::Retrying);
        Timer::after(Duration::from_secs(5)).await;
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface>) {
    runner.run().await
}

/// Errors from [`query_time`].
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TimeError {
    /// The server name didn't resolve.
    Dns,
    /// Socket error.
    Socket,
    /// No reply within 3 s.
    Timeout,
    /// The reply was invalid.
    Sntp(sntp::Error),
}

/// Asks an SNTP server for the time (one UDP round trip, 3 s timeout).
pub async fn query_time(stack: Stack<'static>, server: &str) -> Result<ServerTime, TimeError> {
    let addrs = stack
        .dns_query(server, DnsQueryType::A)
        .await
        .map_err(|_| TimeError::Dns)?;
    let addr = *addrs.first().ok_or(TimeError::Dns)?;

    let (mut rx_meta, mut tx_meta) = ([PacketMetadata::EMPTY; 2], [PacketMetadata::EMPTY; 2]);
    let (mut rx_buf, mut tx_buf) = ([0u8; 128], [0u8; 128]);
    let mut socket = UdpSocket::new(stack, &mut rx_meta, &mut rx_buf, &mut tx_meta, &mut tx_buf);
    socket.bind(0).map_err(|_| TimeError::Socket)?;
    socket
        .send_to(&sntp::request(), (addr, sntp::PORT))
        .await
        .map_err(|_| TimeError::Socket)?;

    let mut reply = [0u8; sntp::PACKET_LEN];
    loop {
        let (n, meta) = with_timeout(Duration::from_secs(3), socket.recv_from(&mut reply))
            .await
            .map_err(|_| TimeError::Timeout)?
            .map_err(|_| TimeError::Socket)?;
        // Ignore stray datagrams from anyone but the server we asked.
        if meta.endpoint.addr == addr {
            return sntp::parse_reply(&reply[..n]).map_err(TimeError::Sntp);
        }
    }
}

/// Wall-clock time anchored to the monotonic clock at one SNTP sample.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct WallClock {
    /// Unix time in microseconds at monotonic instant zero (boot).
    unix_us_at_boot: i64,
}

impl WallClock {
    /// Anchors `t`, as received now.
    pub fn from_server_time(t: ServerTime) -> Self {
        let unix_us = t.unix_seconds as i64 * 1_000_000 + i64::from(t.micros);
        Self {
            unix_us_at_boot: unix_us - Instant::now().as_micros() as i64,
        }
    }

    /// Seconds since the Unix epoch.
    pub fn unix_seconds(&self) -> i64 {
        (self.unix_us_at_boot + Instant::now().as_micros() as i64).div_euclid(1_000_000)
    }

    /// Calendar date and time at `offset_minutes` east of UTC.
    pub fn now(&self, offset_minutes: i32) -> DateTime {
        DateTime::from_unix(self.unix_seconds(), offset_minutes)
    }
}

/// [`query_time`] and anchor the result.
pub async fn sync_clock(stack: Stack<'static>, server: &str) -> Result<WallClock, TimeError> {
    Ok(WallClock::from_server_time(
        query_time(stack, server).await?,
    ))
}
