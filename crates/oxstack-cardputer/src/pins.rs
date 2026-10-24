//! Cardputer ADV pin assignment, for reference and for code that drives
//! pins directly. The typed equivalents live in [`Board`](crate::Board).
//!
//! Source: M5Stack *Sch_M5CardputerAdv_v1.0* (2025-06-20), cross-checked
//! against M5Unified/M5GFX.
//!
//! Avoid UART0. Its default pins on the ESP32-S3 are GPIO43/44, which this
//! board uses for I²S word-select and the IR LED. Log over USB-Serial-JTAG.

/// ST7789V2 LCD on SPI, write-only (3-wire, no MISO).
pub mod lcd {
    /// Reset, active low.
    pub const RST: u8 = 33;
    /// Data/command select.
    pub const DC: u8 = 34;
    /// SPI MOSI.
    pub const MOSI: u8 = 35;
    /// SPI clock.
    pub const SCK: u8 = 36;
    /// Chip select, active low.
    pub const CS: u8 = 37;
    /// Backlight enable / PWM. **Also gates power to the RGB LED.**
    pub const BL: u8 = 38;
}

/// Internal I²C bus shared by the keyboard scanner, audio codec and IMU.
pub mod sys_i2c {
    /// SDA.
    pub const SDA: u8 = 8;
    /// SCL.
    pub const SCL: u8 = 9;
    /// TCA8418 keypad scanner.
    pub const ADDR_TCA8418: u8 = 0x34;
    /// ES8311 audio codec (CE tied low).
    pub const ADDR_ES8311: u8 = 0x18;
    /// BMI270 IMU (SDO pulled high).
    pub const ADDR_BMI270: u8 = 0x69;
}

/// TCA8418 interrupt output, active low.
pub const KEYBOARD_INT: u8 = 11;

/// ES8311 codec I²S. There is no MCLK line; the codec clocks itself from BCLK.
pub mod i2s {
    /// Bit clock (codec SCLK).
    pub const BCLK: u8 = 41;
    /// Word select (codec LRCK).
    pub const WS: u8 = 43;
    /// Data to the codec DAC (codec DSDIN).
    pub const DOUT: u8 = 42;
    /// Data from the codec ADC (codec ASDOUT). Strapping pin, input only here.
    pub const DIN: u8 = 46;
}

/// SPI bus shared by the microSD slot and the EXT header (e.g. LoRa cap).
pub mod spi_bus {
    /// Clock (SD CLK).
    pub const SCK: u8 = 40;
    /// MOSI (SD CMD).
    pub const MOSI: u8 = 14;
    /// MISO (SD D0).
    pub const MISO: u8 = 39;
    /// microSD chip select (SD D3).
    pub const SD_CS: u8 = 12;
    /// EXT header chip select.
    pub const EXT_CS: u8 = 5;
}

/// 14-pin EXT header (besides the shared SPI bus and system I²C).
pub mod ext {
    /// RESET line to the module.
    pub const RESET: u8 = 3;
    /// Interrupt from the module.
    pub const INT: u8 = 4;
    /// BUSY from the module.
    pub const BUSY: u8 = 6;
    /// UART TX from the ESP32 (header pin labelled GPS-RX).
    pub const UART_TX: u8 = 13;
    /// UART RX into the ESP32 (header pin labelled GPS-TX).
    pub const UART_RX: u8 = 15;
}

/// Grove HY2.0-4P port (5 V switchable to input).
pub mod grove {
    /// Yellow wire. M5Unified uses it as external-I²C SCL.
    pub const G1: u8 = 1;
    /// White wire. M5Unified uses it as external-I²C SDA.
    pub const G2: u8 = 2;
}

/// WS2812-class RGB LED data. Powered only while [`lcd::BL`] is high.
pub const RGB_LED: u8 = 21;
/// IR LED, driven directly through 22 Ω (no carrier hardware).
pub const IR_TX: u8 = 44;
/// Battery voltage through a 100 k / 100 k divider (×2), ADC1.
pub const BATTERY_ADC: u8 = 10;
/// G0 button; hold at power-on for download mode. Active low.
pub const BUTTON_G0: u8 = 0;
/// Spare line on the internal 8-pin FPC connector.
pub const FPC_IO1: u8 = 7;
