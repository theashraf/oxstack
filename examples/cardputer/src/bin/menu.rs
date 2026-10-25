//! A tiny app menu: proof that drivers can be created, dropped and created
//! again without a reboot. Each app borrows the hardware it needs from the
//! board and hands it back when it returns.
//!
//! - `1` microphone level for two seconds (silent)
//! - `2` a short beep at low volume (uses the same codec and I²S as `1`)
//! - `3` list the SD card's root directory (read-only)
//! - `4` cycle the RGB LED and send one IR frame
//! - `5` read the IMU
//! - `Esc` leaves an app early
//!
//! Each app's run count is shown on the menu.
#![no_std]
#![no_main]

use core::ops::ControlFlow;

use defmt::{info, warn};
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Timer, with_timeout};
use embedded_graphics::mono_font::MonoTextStyleBuilder;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text};
use esp_backtrace as _;
use esp_println as _;
use oxstack_cardputer::audio::{AudioResources, MicGain, SampleRate, Volume};
use oxstack_cardputer::display::{Display, Frame};
use oxstack_cardputer::dsp::level::to_dbfs;
use oxstack_cardputer::dsp::tone::Sine;
use oxstack_cardputer::gfx::text;
use oxstack_cardputer::i2c::SysI2cBus;
use oxstack_cardputer::imu::{self, ImuResources};
use oxstack_cardputer::input::Action;
use oxstack_cardputer::keyboard::Keyboard;
use oxstack_cardputer::rmt::RmtResources;
use oxstack_cardputer::sd::FixedTime;
use oxstack_cardputer::sd::fs::{VolumeIdx, VolumeManager};
use oxstack_cardputer::{self as bsp, SpiBusResources, display};

esp_bootloader_esp_idf::esp_app_desc!();

const RATE: SampleRate = SampleRate::Hz48000;
const APPS: [&str; 5] = ["Mic level", "Beep", "SD listing", "LED + IR", "IMU"];

/// Screen plus keyboard: what every app gets.
struct Ui<'a, 'd> {
    lcd: Display<'d>,
    fb: &'a mut Frame,
    keyboard: Keyboard<'d>,
}

impl Ui<'_, '_> {
    fn clear(&mut self, title: &str) {
        self.fb.clear(Rgb565::BLACK).unwrap();
        self.line(0, title, Rgb565::CSS_ORANGE);
    }

    fn line(&mut self, row: i32, s: &str, color: Rgb565) {
        let style = MonoTextStyleBuilder::new()
            .font(&FONT_6X10)
            .text_color(color)
            .background_color(Rgb565::BLACK)
            .build();
        let mut l = text!(40, "{}", s);
        Text::with_baseline(l.pad(40), Point::new(2, 10 * row), style, Baseline::Top)
            .draw(self.fb)
            .unwrap();
    }

    async fn flush(&mut self) {
        self.lcd.flush(self.fb).await.unwrap();
    }

    /// Waits up to `d` for Esc; `true` if it was pressed.
    async fn escape_within(&mut self, d: Duration) -> bool {
        let deadline = Timer::after(d);
        let keys = async {
            loop {
                if let Ok(ev) = self.keyboard.next_event().await
                    && ev.pressed
                    && ev.action() == Some(Action::Escape)
                {
                    return;
                }
            }
        };
        matches!(select(deadline, keys).await, Either::Second(()))
    }
}

async fn mic(audio: &mut AudioResources<'_>, bus: &'static SysI2cBus, ui: &mut Ui<'_, '_>) {
    ui.clear("Mic level (2 s, silent)");
    let mut mic = audio.recorder(bus, RATE, MicGain::DB0).await.unwrap();
    let mut buf = [0i16; 1024];
    for row in 2..10 {
        let mut peak = 0i16;
        let mut n = 0;
        // ~250 ms of audio per row.
        while n < RATE.hz() as usize / 2 {
            let got = mic.read(&mut buf).await.unwrap_or(0);
            peak = buf[..got]
                .iter()
                .fold(peak, |p, &s| p.max(s.saturating_abs()));
            n += got;
        }
        let dbfs = to_dbfs(f32::from(peak) / 32768.0);
        ui.line(row, &text!(40, "peak {:>6.1} dBFS", dbfs), Rgb565::GREEN);
        ui.flush().await;
    }
    mic.shutdown().await.unwrap(); // codec off before the stream stops
}

async fn beep(audio: &mut AudioResources<'_>, bus: &'static SysI2cBus, ui: &mut Ui<'_, '_>) {
    ui.clear("Beep");
    ui.line(2, "440 Hz, -30 dB, 300 ms", Rgb565::WHITE);
    ui.flush().await;
    let mut player = audio
        .player(bus, RATE, Volume::from_half_db(-60))
        .await
        .unwrap();
    let mut osc = Sine::new(440.0, RATE.hz(), 0.5);
    let mut block = [0i16; 512];
    for _ in 0..(RATE.hz() as usize * 3 / 10 / 256) {
        osc.fill(&mut block, 2);
        let _ = player.write(&block).await;
    }
    player.shutdown().await.unwrap(); // mute, codec off, then stop
}

async fn sd(spi_bus: &mut SpiBusResources<'_>, ui: &mut Ui<'_, '_>) {
    ui.clear("SD root (read-only)");
    let sd = match spi_bus.sd() {
        Ok(sd) => sd,
        Err(e) => {
            warn!("SD: {}", e);
            ui.line(2, "no card / no response", Rgb565::RED);
            ui.flush().await;
            return;
        }
    };
    let fs = VolumeManager::<_, _, 4, 4, 1>::new(sd, FixedTime);
    let Ok(volume) = fs.open_volume(VolumeIdx(0)) else {
        ui.line(2, "no FAT volume", Rgb565::RED);
        ui.flush().await;
        return;
    };
    let root = volume.open_root_dir().unwrap();
    let mut row = 2;
    let _ = root.iterate_dir(|e| {
        if !e.attributes.is_volume() && row < 13 {
            ui.line(
                row,
                &text!(
                    40,
                    "{}{}",
                    e.name,
                    if e.attributes.is_directory() { "/" } else { "" }
                ),
                Rgb565::WHITE,
            );
            row += 1;
        }
        ControlFlow::Continue(())
    });
    ui.flush().await;
    // `fs` (and the card, and the bus) are released here.
}

async fn led_ir(rmt: &mut RmtResources<'_>, ui: &mut Ui<'_, '_>) {
    ui.clear("LED + IR");
    let (mut led, mut ir) = rmt.init().unwrap();
    for (name, r, g, b) in [("red", 32, 0, 0), ("green", 0, 32, 0), ("blue", 0, 0, 32)] {
        ui.line(2, &text!(40, "LED {}", name), Rgb565::WHITE);
        ui.flush().await;
        led.set(r, g, b).await.unwrap();
        Timer::after_millis(400).await;
    }
    led.off().await.unwrap();
    ir.send_nec(0xBE, 0x01).await.unwrap();
    ui.line(3, "IR frame sent", Rgb565::GREEN);
    ui.flush().await;
}

async fn imu_app(imu: &mut ImuResources, bus: &'static SysI2cBus, ui: &mut Ui<'_, '_>) {
    ui.clear("IMU (board frame)");
    let mut imu = imu.init(bus, imu::Config::default()).await.unwrap();
    for row in 2..10 {
        let s = imu.read().await.unwrap();
        let [x, y, z] = s.accel;
        ui.line(
            row,
            &text!(40, "a {:+.2} {:+.2} {:+.2}", x, y, z),
            Rgb565::GREEN,
        );
        ui.flush().await;
        Timer::after_millis(200).await;
    }
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let mut board = bsp::init(bsp::default_config());
    let _backlight = board.backlight.init();
    let bus = board.sys_i2c.into_bus();
    let mut ui = Ui {
        lcd: board.display.init().await.unwrap(),
        fb: display::framebuffer(),
        keyboard: board.keyboard.init(bus).await.unwrap(),
    };
    let mut runs = [0u32; APPS.len()];

    loop {
        ui.clear("oxstack menu   (1-5, Esc)");
        for (i, name) in APPS.iter().enumerate() {
            ui.line(
                2 + i as i32,
                &text!(40, "{}  {:<12} runs: {}", i + 1, name, runs[i]),
                Rgb565::WHITE,
            );
        }
        ui.flush().await;

        let Ok(ev) = ui.keyboard.next_event().await else {
            continue;
        };
        let Some(Action::Char(c @ '1'..='5')) = ev.action().filter(|_| ev.pressed) else {
            continue;
        };
        let app = (c as u8 - b'1') as usize;
        info!("running {}", APPS[app]);
        // Every app borrows one resource group; the borrow ends when the
        // app returns, so the next run (or another app) can use it again.
        match app {
            0 => mic(&mut board.audio, bus, &mut ui).await,
            1 => beep(&mut board.audio, bus, &mut ui).await,
            2 => sd(&mut board.spi_bus, &mut ui).await,
            3 => led_ir(&mut board.rmt, &mut ui).await,
            _ => imu_app(&mut board.imu, bus, &mut ui).await,
        }
        runs[app] += 1;
        ui.line(12, "Esc / any key: back", Rgb565::CSS_GRAY);
        ui.flush().await;
        let _ = with_timeout(
            Duration::from_secs(5),
            ui.escape_within(Duration::from_secs(5)),
        )
        .await;
    }
}
