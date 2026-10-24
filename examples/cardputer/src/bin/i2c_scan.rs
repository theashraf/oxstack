//! Lists every device on the internal I²C bus.
//!
//! Expected on a Cardputer ADV: 0x18 (ES8311), 0x34 (TCA8418), 0x69 (BMI270).
//! Run this first on new hardware to confirm the bus and addresses.
#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::Timer;
use embedded_hal_async::i2c::I2c as _;
use esp_backtrace as _;
use esp_println as _;
use oxstack_cardputer::pins::sys_i2c;
use oxstack_cardputer::{self as bsp, i2c};

esp_bootloader_esp_idf::esp_app_desc!();

fn name(addr: u8) -> &'static str {
    match addr {
        sys_i2c::ADDR_ES8311 => "ES8311 audio codec",
        sys_i2c::ADDR_TCA8418 => "TCA8418 keyboard scanner",
        sys_i2c::ADDR_BMI270 => "BMI270 IMU",
        _ => "unknown",
    }
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let board = bsp::init(bsp::default_config());
    let mut dev = i2c::device(board.sys_i2c.into_bus());

    loop {
        info!("scanning internal I2C (SDA=G8, SCL=G9)");
        let mut found = 0;
        for addr in 0x08..=0x77u8 {
            // An address-only write: ACK means a device is present.
            if dev.write(addr, &[]).await.is_ok() {
                info!("  0x{=u8:02x}  {}", addr, name(addr));
                found += 1;
            }
        }
        info!("{} device(s) found", found);
        Timer::after_secs(5).await;
    }
}
