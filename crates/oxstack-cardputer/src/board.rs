use esp_hal::peripherals::*;

/// Every peripheral of the chip, grouped by what the Cardputer ADV wires it
/// to. Create it with [`crate::init`] or [`Board::new`].
///
/// Each group's `init` borrows the group, so take the board as `mut` and
/// keep it for the life of the program; drivers are created from it and
/// dropped as apps come and go.
#[non_exhaustive]
pub struct Board {
    /// ST7789V2 LCD (SPI3 + DMA channel 0).
    pub display: crate::display::DisplayResources<'static>,
    /// LCD backlight, which also powers the RGB LED.
    pub backlight: crate::backlight::BacklightResources<'static>,
    /// Internal I²C bus (keyboard, codec, IMU).
    pub sys_i2c: crate::i2c::SysI2cResources<'static>,
    /// Keyboard scanner interrupt line (the scanner itself is on `sys_i2c`).
    pub keyboard: crate::keyboard::KeyboardResources<'static>,
    /// ES8311 codec I²S link.
    pub audio: crate::audio::AudioResources<'static>,
    /// SPI bus shared by microSD and the EXT header.
    pub spi_bus: SpiBusResources<'static>,
    /// EXT header control and UART lines.
    pub ext: ExtResources<'static>,
    /// RMT pulse generator with the RGB LED and IR LED it drives.
    pub rmt: crate::rmt::RmtResources<'static>,
    /// Battery voltage sense.
    pub battery: crate::battery::BatteryResources<'static>,
    /// BMI270 IMU (on `sys_i2c`).
    pub imu: crate::imu::ImuResources,
    /// G0 button (active low). Read it with
    /// `Input::new(board.button.reborrow(), InputConfig::default().with_pull(Pull::Up))`.
    pub button: GPIO0<'static>,
    /// Grove port.
    pub grove: GroveResources<'static>,
    /// Spare line on the internal FPC connector.
    pub fpc_io1: GPIO7<'static>,
    /// Wi-Fi and Bluetooth radio.
    pub radio: crate::radio::RadioResources<'static>,
    /// Peripherals not tied to a board function.
    pub system: SystemResources<'static>,
    /// GPIOs and UART0 that the board leaves unrouted.
    pub spare: SpareResources<'static>,
    /// Register-block peripherals for low-level or `unsafe` work.
    pub raw: RawResources<'static>,
}

/// Pins the Cardputer ADV schematic does not route to any connector, plus
/// UART0. Kept rather than dropped so nothing is lost for good.
#[allow(missing_docs)]
pub struct SpareResources<'d> {
    pub gpio16: GPIO16<'d>,
    pub gpio17: GPIO17<'d>,
    pub gpio18: GPIO18<'d>,
    pub gpio45: GPIO45<'d>,
    pub gpio47: GPIO47<'d>,
    pub gpio48: GPIO48<'d>,
    /// **Don't use on its default pins** (GPIO43/44 are taken here); see
    /// [`crate::pins`].
    pub uart0: UART0<'d>,
}
reborrow_group!(SpareResources {
    gpio16,
    gpio17,
    gpio18,
    gpio45,
    gpio47,
    gpio48,
    uart0
});

/// Peripherals that are mostly register blocks, for low-level code.
#[allow(missing_docs)]
pub struct RawResources<'d> {
    pub apb_ctrl: APB_CTRL<'d>,
    pub assist_debug: ASSIST_DEBUG<'d>,
    pub dma: DMA<'d>,
    pub extmem: EXTMEM<'d>,
    pub gpio: GPIO<'d>,
    pub i2c_ana_mst: I2C_ANA_MST<'d>,
    pub interrupt_core0: INTERRUPT_CORE0<'d>,
    pub interrupt_core1: INTERRUPT_CORE1<'d>,
    pub rtc_timer: RTC_TIMER<'d>,
    pub peri_backup: PERI_BACKUP<'d>,
    pub rtc_cntl: RTC_CNTL<'d>,
    pub sens: SENS<'d>,
    pub sensitive: SENSITIVE<'d>,
    pub system: SYSTEM<'d>,
    pub wcl: WCL<'d>,
}
reborrow_group!(RawResources {
    apb_ctrl,
    assist_debug,
    dma,
    extmem,
    gpio,
    i2c_ana_mst,
    interrupt_core0,
    interrupt_core1,
    rtc_timer,
    peri_backup,
    rtc_cntl,
    sens,
    sensitive,
    system,
    wcl
});

/// The shared SPI bus and both of its chip selects. [`sd`](Self::sd) is in
/// [`crate::sd`].
#[allow(missing_docs)]
pub struct SpiBusResources<'d> {
    pub spi: SPI2<'d>,
    pub dma: DMA_CH2<'d>,
    pub sck: GPIO40<'d>,
    pub mosi: GPIO14<'d>,
    pub miso: GPIO39<'d>,
    pub sd_cs: GPIO12<'d>,
    pub ext_cs: GPIO5<'d>,
}
reborrow_group!(SpiBusResources {
    spi,
    dma,
    sck,
    mosi,
    miso,
    sd_cs,
    ext_cs
});

/// EXT header lines other than SPI and I²C.
#[allow(missing_docs)]
pub struct ExtResources<'d> {
    pub reset: GPIO3<'d>,
    pub int: GPIO4<'d>,
    pub busy: GPIO6<'d>,
    pub uart: UART1<'d>,
    pub uart_tx: GPIO13<'d>,
    pub uart_rx: GPIO15<'d>,
}
reborrow_group!(ExtResources {
    reset,
    int,
    busy,
    uart,
    uart_tx,
    uart_rx
});

/// Grove port signal lines.
#[allow(missing_docs)]
pub struct GroveResources<'d> {
    pub g1: GPIO1<'d>,
    pub g2: GPIO2<'d>,
}
reborrow_group!(GroveResources { g1, g2 });

/// Chip peripherals with no fixed board role.
#[allow(missing_docs)]
pub struct SystemResources<'d> {
    pub timg1: TIMG1<'d>,
    pub systimer: SYSTIMER<'d>,
    pub i2c1: I2C1<'d>,
    pub i2s1: I2S1<'d>,
    pub uart2: UART2<'d>,
    pub adc2: ADC2<'d>,
    pub dma_ch3: DMA_CH3<'d>,
    pub dma_ch4: DMA_CH4<'d>,
    pub rng: RNG<'d>,
    pub usb_device: USB_DEVICE<'d>,
    pub lpwr: LPWR<'d>,
    pub cpu_ctrl: CPU_CTRL<'d>,
    pub sw_int1: FROM_CPU_INTR1<'d>,
    pub sw_int2: FROM_CPU_INTR2<'d>,
    pub sw_int3: FROM_CPU_INTR3<'d>,
    pub pcnt: PCNT<'d>,
    pub mcpwm0: MCPWM0<'d>,
    pub mcpwm1: MCPWM1<'d>,
    pub sha: SHA<'d>,
    pub aes: AES<'d>,
    pub rsa: RSA<'d>,
    pub hmac: HMAC<'d>,
    pub sdhost: SDHOST<'d>,
    pub twai: TWAI0<'d>,
    pub usb_otg: USB_FS<'d>,
    pub lcd_cam: LCD_CAM<'d>,
    pub flash: FLASH<'d>,
    pub io_mux: IO_MUX<'d>,
    pub rtc_io: RTC_IO<'d>,
    pub ulp_riscv_core: ULP_RISCV_CORE<'d>,
    pub lp_i2c0: LP_I2C0<'d>,
    pub gpio_dedicated: GPIO_DEDICATED<'d>,
    pub apb_saradc: APB_SARADC<'d>,
    pub uhci0: UHCI0<'d>,
    pub ds: DS<'d>,
    pub xts_aes: XTS_AES<'d>,
    pub gpio_sd: GPIO_SD<'d>,
    pub sdm_ch0: SDM_CH0<'d>,
    pub sdm_ch1: SDM_CH1<'d>,
    pub sdm_ch2: SDM_CH2<'d>,
    pub sdm_ch3: SDM_CH3<'d>,
    pub sdm_ch4: SDM_CH4<'d>,
    pub sdm_ch5: SDM_CH5<'d>,
    pub sdm_ch6: SDM_CH6<'d>,
    pub sdm_ch7: SDM_CH7<'d>,
}
reborrow_group!(SystemResources {
    timg1,
    systimer,
    i2c1,
    i2s1,
    uart2,
    adc2,
    dma_ch3,
    dma_ch4,
    rng,
    usb_device,
    lpwr,
    cpu_ctrl,
    sw_int1,
    sw_int2,
    sw_int3,
    pcnt,
    mcpwm0,
    mcpwm1,
    sha,
    aes,
    rsa,
    hmac,
    sdhost,
    twai,
    usb_otg,
    lcd_cam,
    flash,
    io_mux,
    rtc_io,
    ulp_riscv_core,
    lp_i2c0,
    gpio_dedicated,
    apb_saradc,
    uhci0,
    ds,
    xts_aes,
    gpio_sd,
    sdm_ch0,
    sdm_ch1,
    sdm_ch2,
    sdm_ch3,
    sdm_ch4,
    sdm_ch5,
    sdm_ch6,
    sdm_ch7
});

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
            rmt: crate::rmt::RmtResources {
                rmt: p.RMT,
                rgb_led: p.GPIO21,
                ir_tx: p.GPIO44,
            },
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
