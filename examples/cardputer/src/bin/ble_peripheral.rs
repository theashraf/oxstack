//! Bluetooth LE peripheral "oxstack" with two GATT services:
//!
//! - **Battery** (`0x180F`): level in %, estimated from the cell voltage;
//!   read + notify (every 30 s while connected).
//! - **oxstack** (`4f58a000-6b1f-4d8e-9a3b-3c1e5a7d2b10`):
//!   - `key` (`…a001`, read + notify): ASCII code of each character typed on
//!     the Cardputer keyboard.
//!   - `led` (`…a002`, read + write): 3 bytes `[r, g, b]` that set the RGB LED.
//!
//! Advertising restarts after every disconnect. Try it with any BLE app
//! (nRF Connect, LightBlue) or `bleak` on a computer.
//!
//! Run with `cargo run --release --bin ble_peripheral --features ble`.
#![no_std]
#![no_main]

use core::fmt::Write as _;

use defmt::{info, warn};
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Timer};
use embedded_graphics::mono_font::MonoTextStyleBuilder;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text};
use esp_backtrace as _;
use esp_println as _;
use oxstack_cardputer::battery::Battery;
use oxstack_cardputer::ble::{self, trouble_host};
use oxstack_cardputer::gfx::TextBuf;
use oxstack_cardputer::input::Action;
use oxstack_cardputer::keyboard::Keyboard;
use oxstack_cardputer::led::RgbLed;
use oxstack_cardputer::{self as bsp, display};
use trouble_host::prelude::*;

esp_bootloader_esp_idf::esp_app_desc!();

const NAME: &str = "oxstack";

#[gatt_server]
struct Server {
    battery: BatteryService,
    oxstack: OxstackService,
}

#[gatt_service(uuid = service::BATTERY)]
struct BatteryService {
    #[characteristic(uuid = characteristic::BATTERY_LEVEL, read, notify)]
    level: u8,
}

#[gatt_service(uuid = "4f58a000-6b1f-4d8e-9a3b-3c1e5a7d2b10")]
struct OxstackService {
    /// ASCII code of the last character typed.
    #[characteristic(uuid = "4f58a001-6b1f-4d8e-9a3b-3c1e5a7d2b10", read, notify)]
    key: u8,
    /// RGB LED colour.
    #[characteristic(uuid = "4f58a002-6b1f-4d8e-9a3b-3c1e5a7d2b10", read, write)]
    led: [u8; 3],
}

struct Ui<'a> {
    lcd: display::Display<'a>,
    fb: &'a mut display::Frame,
}

impl Ui<'_> {
    async fn line(&mut self, y: i32, args: core::fmt::Arguments<'_>, color: Rgb565) {
        let mut l = TextBuf::<38>::new();
        let _ = l.write_fmt(args);
        l.pad(38); // overwrite whatever longer text was there
        let style = MonoTextStyleBuilder::new()
            .font(&FONT_6X10)
            .text_color(color)
            .background_color(Rgb565::BLACK)
            .build();
        Text::with_baseline(l.as_str(), Point::new(2, y), style, Baseline::Top)
            .draw(self.fb)
            .unwrap();
        self.lcd.flush(self.fb).await.unwrap();
    }
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let mut board = bsp::init(bsp::default_config());
    let _backlight = board.backlight.init(); // also powers the RGB LED
    let bus = board.sys_i2c.into_bus();
    let mut ui = Ui {
        lcd: board.display.init().await.unwrap(),
        fb: display::framebuffer(),
    };
    let mut keyboard = board.keyboard.init(bus).await.unwrap();
    let mut battery = board.battery.init();
    let (mut led, _ir) = board.rmt.init().unwrap();
    let _ = led.off().await;

    let address = ble::address();
    let controller = ble::controller(board.radio.bt).unwrap();
    let mut resources: HostResources<_, DefaultPacketPool, 1, 2> = HostResources::new();
    let stack = ble::stack(controller, &mut resources);
    let mut runner = stack.runner();
    let mut peripheral = stack.peripheral();
    let server = Server::new_with_config(GapConfig::Peripheral(PeripheralConfig {
        name: NAME,
        appearance: &appearance::human_interface_device::KEYBOARD,
    }))
    .unwrap();
    let _ = server.set(&server.battery.level, &battery.percent().await);

    ui.fb.clear(Rgb565::BLACK).unwrap();
    ui.line(
        0,
        format_args!("oxstack BLE peripheral"),
        Rgb565::CSS_ORANGE,
    )
    .await;
    ui.line(12, format_args!("addr {}", Fmt(address)), Rgb565::WHITE)
        .await;

    let app = async {
        loop {
            ui.line(
                30,
                format_args!("advertising as \"{}\"", NAME),
                Rgb565::WHITE,
            )
            .await;
            let conn = match advertise(&mut peripheral, &server).await {
                Ok(conn) => conn,
                Err(e) => {
                    warn!("advertising failed: {}", defmt::Debug2Format(&e));
                    Timer::after_secs(1).await;
                    continue;
                }
            };
            info!("connected: {}", conn.raw().peer_address());
            ui.line(
                30,
                format_args!("connected {}", Fmt(conn.raw().peer_address())),
                Rgb565::GREEN,
            )
            .await;
            select(
                gatt_events(&server, &conn, &mut led),
                app_task(&server, &conn, &mut keyboard, &mut battery, &mut ui),
            )
            .await;
            info!("disconnected");
            let _ = led.off().await;
        }
    };
    join(
        async {
            loop {
                if let Err(e) = runner.run().await {
                    warn!("BLE runner error: {}", defmt::Debug2Format(&e));
                }
            }
        },
        app,
    )
    .await;
    unreachable!()
}

/// Formats a BLE address as `AA:BB:CC:DD:EE:FF`.
struct Fmt<A>(A);

impl core::fmt::Display for Fmt<Address> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Fmt(self.0.addr).fmt(f)
    }
}

impl core::fmt::Display for Fmt<BdAddr> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let b = self.0.raw();
        for (i, byte) in b.iter().rev().enumerate() {
            if i > 0 {
                f.write_str(":")?;
            }
            write!(f, "{byte:02X}")?;
        }
        Ok(())
    }
}

async fn advertise<'v, 's, C: Controller>(
    peripheral: &mut Peripheral<'v, C, DefaultPacketPool>,
    server: &'s Server<'v>,
) -> Result<GattConnection<'v, 's, DefaultPacketPool>, BleHostError<C::Error>> {
    let mut adv = [0; 31];
    let len = AdStructure::encode_slice(
        &[
            AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
            AdStructure::IncompleteServiceUuids16(&[[0x0f, 0x18]]),
            AdStructure::CompleteLocalName(NAME.as_bytes()),
        ],
        &mut adv[..],
    )?;
    let advertiser = peripheral
        .advertise(
            &Default::default(),
            Advertisement::ConnectableScannableUndirected {
                adv_data: &adv[..len],
                scan_data: &[],
            },
        )
        .await?;
    Ok(advertiser.accept().await?.with_attribute_server(server)?)
}

/// Serves GATT requests until the central disconnects. Writes to `led`
/// change the RGB LED.
async fn gatt_events(
    server: &Server<'_>,
    conn: &GattConnection<'_, '_, DefaultPacketPool>,
    led: &mut RgbLed<'_>,
) {
    let led_handle = server.oxstack.led.handle;
    loop {
        match conn.next().await {
            GattConnectionEvent::Disconnected { reason } => {
                info!("disconnect reason: {}", defmt::Debug2Format(&reason));
                return;
            }
            GattConnectionEvent::Gatt { event } => {
                let wrote_led = matches!(&event, GattEvent::Write(w) if w.handle() == led_handle);
                match event.accept() {
                    Ok(reply) => reply.send().await,
                    Err(e) => warn!("GATT reply error: {}", defmt::Debug2Format(&e)),
                }
                if wrote_led && let Ok([r, g, b]) = server.get(&server.oxstack.led) {
                    info!("LED set over BLE to ({}, {}, {})", r, g, b);
                    if let Err(e) = led.set(r, g, b).await {
                        warn!("LED error: {}", e);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Streams typed characters as `key` notifications and refreshes the
/// battery level every 30 s. Returns when a notification fails (the link
/// is gone).
async fn app_task(
    server: &Server<'_>,
    conn: &GattConnection<'_, '_, DefaultPacketPool>,
    keyboard: &mut Keyboard<'_>,
    battery: &mut Battery<'_>,
    ui: &mut Ui<'_>,
) {
    let mut sent = 0u32;
    loop {
        match select(keyboard.next_event(), Timer::after(Duration::from_secs(30))).await {
            Either::First(Ok(ev)) => {
                if let Some(Action::Char(c)) = ev.action()
                    && c.is_ascii()
                {
                    if server
                        .oxstack
                        .key
                        .notify(conn, &(c as u8), true)
                        .await
                        .is_err()
                    {
                        return;
                    }
                    sent += 1;
                    info!("sent key '{}' ({} total)", c, sent);
                    ui.line(
                        50,
                        format_args!("keys sent: {}  last '{}'", sent, c),
                        Rgb565::WHITE,
                    )
                    .await;
                }
            }
            Either::First(Err(_)) => warn!("keyboard error"),
            Either::Second(()) => {
                let pct = battery.percent().await;
                if server.battery.level.notify(conn, &pct, true).await.is_err() {
                    return;
                }
                ui.line(62, format_args!("battery ~{}%", pct), Rgb565::WHITE)
                    .await;
            }
        }
    }
}
