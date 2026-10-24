use esp_hal::peripherals::*;

/// Every peripheral of the chip, grouped by what the Cardputer ADV wires it
/// to. Create it with [`crate::init`] or [`Board::new`].
#[non_exhaustive]
pub struct Board {
    /// ST7789V2 LCD (SPI3 + DMA channel 0).
    pub display: crate::display::DisplayResources,
    /// LCD backlight, which also powers the RGB LED.
    pub backlight: crate::backlight::BacklightResources,
    /// Internal I²C bus (keyboard, codec, IMU).
    pub sys_i2c: crate::i2c::SysI2cResources,
    /// Keyboard scanner interrupt line (the scanner itself is on `sys_i2c`).
    pub keyboard: crate::keyboard::KeyboardResources,
    /// ES8311 codec I²S link.
    pub audio: crate::audio::AudioResources,
    /// SPI bus shared by microSD and the EXT header.
    pub spi_bus: SpiBusResources,
    /// EXT header control and UART lines.
    pub ext: ExtResources,
    /// RGB LED data line.
    pub rgb_led: GPIO21<'static>,
    /// IR LED.
    pub ir_tx: GPIO44<'static>,
    /// Battery voltage sense.
    pub battery: crate::battery::BatteryResources,
    /// BMI270 IMU (on `sys_i2c`).
    pub imu: crate::imu::ImuResources,
    /// G0 button (active low).
    pub button: GPIO0<'static>,
    /// Grove port.
    pub grove: GroveResources,
    /// Spare line on the internal FPC connector.
    pub fpc_io1: GPIO7<'static>,
    /// Wi-Fi and Bluetooth radio.
    pub radio: crate::radio::RadioResources,
    /// Peripherals not tied to a board function.
    pub system: SystemResources,
    /// GPIOs and UART0 that the board leaves unrouted.
    pub spare: SpareResources,
    /// Register-block peripherals for low-level or `unsafe` work.
    pub raw: RawResources,
}

/// Pins the Cardputer ADV schematic does not route to any connector, plus
/// UART0. Kept rather than dropped so nothing is lost for good.
#[allow(missing_docs)]
pub struct SpareResources {
    pub gpio16: GPIO16<'static>,
    pub gpio17: GPIO17<'static>,
    pub gpio18: GPIO18<'static>,
    pub gpio45: GPIO45<'static>,
    pub gpio47: GPIO47<'static>,
    pub gpio48: GPIO48<'static>,
    /// **Don't use on its default pins** (GPIO43/44 are taken here); see
    /// [`crate::pins`].
    pub uart0: UART0<'static>,
}

/// Peripherals that are mostly register blocks, for low-level code.
#[allow(missing_docs)]
pub struct RawResources {
    pub apb_ctrl: APB_CTRL<'static>,
    pub assist_debug: ASSIST_DEBUG<'static>,
    pub dma: DMA<'static>,
    pub extmem: EXTMEM<'static>,
    pub gpio: GPIO<'static>,
    pub i2c_ana_mst: I2C_ANA_MST<'static>,
    pub interrupt_core0: INTERRUPT_CORE0<'static>,
    pub interrupt_core1: INTERRUPT_CORE1<'static>,
    pub rtc_timer: RTC_TIMER<'static>,
    pub peri_backup: PERI_BACKUP<'static>,
    pub rtc_cntl: RTC_CNTL<'static>,
    pub sens: SENS<'static>,
    pub sensitive: SENSITIVE<'static>,
    pub system: SYSTEM<'static>,
    pub wcl: WCL<'static>,
}

/// The shared SPI bus and both of its chip selects.
#[allow(missing_docs)]
pub struct SpiBusResources {
    pub spi: SPI2<'static>,
    pub dma: DMA_CH2<'static>,
    pub sck: GPIO40<'static>,
    pub mosi: GPIO14<'static>,
    pub miso: GPIO39<'static>,
    pub sd_cs: GPIO12<'static>,
    pub ext_cs: GPIO5<'static>,
}

/// EXT header lines other than SPI and I²C.
#[allow(missing_docs)]
pub struct ExtResources {
    pub reset: GPIO3<'static>,
    pub int: GPIO4<'static>,
    pub busy: GPIO6<'static>,
    pub uart: UART1<'static>,
    pub uart_tx: GPIO13<'static>,
    pub uart_rx: GPIO15<'static>,
}

/// Grove port signal lines.
#[allow(missing_docs)]
pub struct GroveResources {
    pub g1: GPIO1<'static>,
    pub g2: GPIO2<'static>,
}

/// Chip peripherals with no fixed board role.
#[allow(missing_docs)]
pub struct SystemResources {
    pub timg1: TIMG1<'static>,
    pub systimer: SYSTIMER<'static>,
    pub rmt: RMT<'static>,
    pub i2c1: I2C1<'static>,
    pub i2s1: I2S1<'static>,
    pub uart2: UART2<'static>,
    pub adc2: ADC2<'static>,
    pub dma_ch3: DMA_CH3<'static>,
    pub dma_ch4: DMA_CH4<'static>,
    pub rng: RNG<'static>,
    pub usb_device: USB_DEVICE<'static>,
    pub lpwr: LPWR<'static>,
    pub cpu_ctrl: CPU_CTRL<'static>,
    pub sw_int1: FROM_CPU_INTR1<'static>,
    pub sw_int2: FROM_CPU_INTR2<'static>,
    pub sw_int3: FROM_CPU_INTR3<'static>,
    pub pcnt: PCNT<'static>,
    pub mcpwm0: MCPWM0<'static>,
    pub mcpwm1: MCPWM1<'static>,
    pub sha: SHA<'static>,
    pub aes: AES<'static>,
    pub rsa: RSA<'static>,
    pub hmac: HMAC<'static>,
    pub sdhost: SDHOST<'static>,
    pub twai: TWAI0<'static>,
    pub usb_otg: USB_FS<'static>,
    pub lcd_cam: LCD_CAM<'static>,
    pub flash: FLASH<'static>,
    pub io_mux: IO_MUX<'static>,
    pub rtc_io: RTC_IO<'static>,
    pub ulp_riscv_core: ULP_RISCV_CORE<'static>,
    pub lp_i2c0: LP_I2C0<'static>,
    pub gpio_dedicated: GPIO_DEDICATED<'static>,
    pub apb_saradc: APB_SARADC<'static>,
    pub uhci0: UHCI0<'static>,
    pub ds: DS<'static>,
    pub xts_aes: XTS_AES<'static>,
    pub gpio_sd: GPIO_SD<'static>,
    pub sdm_ch0: SDM_CH0<'static>,
    pub sdm_ch1: SDM_CH1<'static>,
    pub sdm_ch2: SDM_CH2<'static>,
    pub sdm_ch3: SDM_CH3<'static>,
    pub sdm_ch4: SDM_CH4<'static>,
    pub sdm_ch5: SDM_CH5<'static>,
    pub sdm_ch6: SDM_CH6<'static>,
    pub sdm_ch7: SDM_CH7<'static>,
}

/// What the `esp-rtos` scheduler needs. [`crate::init`] consumes it.
#[allow(missing_docs)]
pub struct RtosResources {
    pub timg0: TIMG0<'static>,
    pub sw_int0: FROM_CPU_INTR0<'static>,
}

impl RtosResources {
    /// Starts the `esp-rtos` scheduler on TIMG0 timer 0.
    pub fn start(self) {
        let timg0 = esp_hal::timer::timg::TimerGroup::new(self.timg0);
        esp_rtos::start(timg0.timer0, self.sw_int0);
    }
}

impl Board {
    /// Splits `esp_hal::init`'s peripherals into board resource groups.
    ///
    /// `Peripherals` tokens can't be recovered once dropped (short of
    /// `unsafe` stealing), so every peripheral is forwarded except exactly
    /// three: `SPI0`/`SPI1`, which serve the flash cache (using them would
    /// crash execution), and `PSRAM`, which the ESP32-S3FN8 does not have.
    pub fn new(p: Peripherals) -> (Self, RtosResources) {
        let board = Board {
            display: crate::display::DisplayResources {
                spi: p.SPI3,
                dma: p.DMA_CH0,
                sck: p.GPIO36,
                mosi: p.GPIO35,
                cs: p.GPIO37,
                dc: p.GPIO34,
                rst: p.GPIO33,
            },
            backlight: crate::backlight::BacklightResources {
                ledc: p.LEDC,
                pin: p.GPIO38,
            },
            sys_i2c: crate::i2c::SysI2cResources {
                i2c: p.I2C0,
                sda: p.GPIO8,
                scl: p.GPIO9,
            },
            keyboard: crate::keyboard::KeyboardResources { int: p.GPIO11 },
            audio: crate::audio::AudioResources {
                i2s: p.I2S0,
                dma: p.DMA_CH1,
                bclk: p.GPIO41,
                ws: p.GPIO43,
                dout: p.GPIO42,
                din: p.GPIO46,
            },
            spi_bus: SpiBusResources {
                spi: p.SPI2,
                dma: p.DMA_CH2,
                sck: p.GPIO40,
                mosi: p.GPIO14,
                miso: p.GPIO39,
                sd_cs: p.GPIO12,
                ext_cs: p.GPIO5,
            },
            ext: ExtResources {
                reset: p.GPIO3,
                int: p.GPIO4,
                busy: p.GPIO6,
                uart: p.UART1,
                uart_tx: p.GPIO13,
                uart_rx: p.GPIO15,
            },
            rgb_led: p.GPIO21,
            ir_tx: p.GPIO44,
            battery: crate::battery::BatteryResources {
                adc: p.ADC1,
                pin: p.GPIO10,
            },
            imu: crate::imu::ImuResources { _private: () },
            button: p.GPIO0,
            grove: GroveResources {
                g1: p.GPIO1,
                g2: p.GPIO2,
            },
            fpc_io1: p.GPIO7,
            radio: crate::radio::RadioResources {
                wifi: p.WIFI,
                bt: p.BT,
            },
            system: SystemResources {
                timg1: p.TIMG1,
                systimer: p.SYSTIMER,
                rmt: p.RMT,
                i2c1: p.I2C1,
                i2s1: p.I2S1,
                uart2: p.UART2,
                adc2: p.ADC2,
                dma_ch3: p.DMA_CH3,
                dma_ch4: p.DMA_CH4,
                rng: p.RNG,
                usb_device: p.USB_DEVICE,
                lpwr: p.LPWR,
                cpu_ctrl: p.CPU_CTRL,
                sw_int1: p.FROM_CPU_INTR1,
                sw_int2: p.FROM_CPU_INTR2,
                sw_int3: p.FROM_CPU_INTR3,
                pcnt: p.PCNT,
                mcpwm0: p.MCPWM0,
                mcpwm1: p.MCPWM1,
                sha: p.SHA,
                aes: p.AES,
                rsa: p.RSA,
                hmac: p.HMAC,
                sdhost: p.SDHOST,
                twai: p.TWAI0,
                usb_otg: p.USB_FS,
                lcd_cam: p.LCD_CAM,
                flash: p.FLASH,
                io_mux: p.IO_MUX,
                rtc_io: p.RTC_IO,
                ulp_riscv_core: p.ULP_RISCV_CORE,
                lp_i2c0: p.LP_I2C0,
                gpio_dedicated: p.GPIO_DEDICATED,
                apb_saradc: p.APB_SARADC,
                uhci0: p.UHCI0,
                ds: p.DS,
                xts_aes: p.XTS_AES,
                gpio_sd: p.GPIO_SD,
                sdm_ch0: p.SDM_CH0,
                sdm_ch1: p.SDM_CH1,
                sdm_ch2: p.SDM_CH2,
                sdm_ch3: p.SDM_CH3,
                sdm_ch4: p.SDM_CH4,
                sdm_ch5: p.SDM_CH5,
                sdm_ch6: p.SDM_CH6,
                sdm_ch7: p.SDM_CH7,
            },
            spare: SpareResources {
                gpio16: p.GPIO16,
                gpio17: p.GPIO17,
                gpio18: p.GPIO18,
                gpio45: p.GPIO45,
                gpio47: p.GPIO47,
                gpio48: p.GPIO48,
                uart0: p.UART0,
            },
            raw: RawResources {
                apb_ctrl: p.APB_CTRL,
                assist_debug: p.ASSIST_DEBUG,
                dma: p.DMA,
                extmem: p.EXTMEM,
                gpio: p.GPIO,
                i2c_ana_mst: p.I2C_ANA_MST,
                interrupt_core0: p.INTERRUPT_CORE0,
                interrupt_core1: p.INTERRUPT_CORE1,
                rtc_timer: p.RTC_TIMER,
                peri_backup: p.PERI_BACKUP,
                rtc_cntl: p.RTC_CNTL,
                sens: p.SENS,
                sensitive: p.SENSITIVE,
                system: p.SYSTEM,
                wcl: p.WCL,
            },
        };
        (
            board,
            RtosResources {
                timg0: p.TIMG0,
                sw_int0: p.FROM_CPU_INTR0,
            },
        )
    }
}
