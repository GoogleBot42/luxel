//! Board definitions — the identity of each supported board lives HERE
//! (name + defaults), and the few lines of physical wiring live in one
//! clearly-marked section of main.rs (pins are esp-hal *types*, so they
//! can't be table data). Adding a board is a five-minute diff — the
//! recipe with exact snippets is in docs/boards.md.

use crate::leds::Protocol;

#[cfg(feature = "board-c3-devkit")]
mod def {
    use super::*;
    pub const NAME: &str = "ESP32-C3 devkit";
    pub const DEFAULT_PROTOCOL: Protocol = Protocol::Sk9822;
    pub const DEFAULT_PIXEL_COUNT: u32 = 60;
    pub const DEFAULT_DATA_PIN: u8 = 7;
    /// SPI CLK.
    pub const RESERVED_PINS: &[u8] = &[6];
}

#[cfg(feature = "board-pixelblaze-v3")]
mod def {
    use super::*;
    pub const NAME: &str = "Pixelblaze v3 Standard";
    pub const DEFAULT_PROTOCOL: Protocol = Protocol::Sk9822;
    pub const DEFAULT_PIXEL_COUNT: u32 = 300;
    pub const DEFAULT_DATA_PIN: u8 = 23;
    /// SPI CLK (18), status LED (12). The button (32) and the expansion
    /// header (0, 25, 26) stay free for patterns.
    pub const RESERVED_PINS: &[u8] = &[18, 12];
}

#[cfg(feature = "board-athom-music")]
mod def {
    use super::*;
    pub const NAME: &str = "Athom music-reactive WLED controller";
    pub const DEFAULT_PROTOCOL: Protocol = Protocol::Ws2812;
    pub const DEFAULT_PIXEL_COUNT: u32 = 60;
    pub const DEFAULT_DATA_PIN: u8 = 18;
    /// CLK1 (5), strip-power relay (2). The case button (0), IR receiver
    /// (25) and mic pins (32/15/36) are NOT reserved: Luxel leaves them
    /// idle, so a pattern may read them (`digitalRead(0)` is the button).
    pub const RESERVED_PINS: &[u8] = &[5, 2];
}

#[cfg(feature = "board-esp32-generic")]
mod def {
    use super::*;
    pub const NAME: &str = "generic ESP32 (VSPI: CLK 18, DATA 23)";
    pub const DEFAULT_PROTOCOL: Protocol = Protocol::Ws2812;
    pub const DEFAULT_PIXEL_COUNT: u32 = 60;
    pub const DEFAULT_DATA_PIN: u8 = 23;
    /// SPI CLK.
    pub const RESERVED_PINS: &[u8] = &[18];
}

// UNTESTED ON METAL: no S3 on the bench. Wiring is reviewed against the
// ESP32-S3-DevKitC-1 pinout (SPI2/FSPI IO_MUX pins), never lit up.
// With `hub75` the output is a matrix panel instead of a strip: the
// default pixel count is the panel area (4096 on a 64x64 panel — the
// per-board MAX_PIXELS below) and DEFAULT_PROTOCOL is vestigial (the
// driver's wire format is fixed; protocol switches are rejected).
#[cfg(feature = "board-s3-devkit")]
mod def {
    use super::*;
    #[cfg(not(feature = "hub75"))]
    pub const NAME: &str = "ESP32-S3 devkit (untested)";
    #[cfg(feature = "hub75")]
    pub const NAME: &str = "ESP32-S3 devkit + HUB75 panel (untested)";
    pub const DEFAULT_PROTOCOL: Protocol = Protocol::Ws2812;
    #[cfg(not(feature = "hub75"))]
    pub const DEFAULT_PIXEL_COUNT: u32 = 60;
    #[cfg(feature = "hub75")]
    pub const DEFAULT_PIXEL_COUNT: u32 = super::PANEL_PIXELS;
    pub const DEFAULT_DATA_PIN: u8 = 11;
    /// SPI CLK (12); with `hub75`, the 14 panel pins from `hub75_pins!`.
    #[cfg(not(feature = "hub75"))]
    pub const RESERVED_PINS: &[u8] = &[12];
    #[cfg(feature = "hub75")]
    pub const RESERVED_PINS: &[u8] = &[38, 42, 48, 47, 2, 21, 14, 46, 13, 9, 3, 11, 12, 10];
}

// UNTESTED ON METAL: ordered 2026-08-22, bring-up tracked in Gitea #75.
// Seengreat "RGB Matrix HUB75 S3" panel driver board: ESP32-S3-WROOM-1
// (16 MB flash / 8 MB octal PSRAM) with two HUB75 outputs (ribbon +
// direct plug-in header), an ES7210/ES8311 codec, microSD and an RTC.
// Luxel uses the HUB75 side only — the codec, SD and RTC are unwired
// (sound-reactive work on this board is Gitea #142). PSRAM is not
// initialised: nothing here needs it, and DMA framebuffers must live in
// internal SRAM anyway; using it as a pattern-array arena is a future
// idea (docs/boards.md).
#[cfg(feature = "board-seengreat-hub75")]
mod def {
    use super::*;
    pub const NAME: &str = "Seengreat RGB Matrix HUB75 S3 (untested)";
    // Vestigial: the panel driver's wire format is fixed and
    // set_protocol() rejects switches. Kept because the field is part of
    // the persisted device config on every board.
    pub const DEFAULT_PROTOCOL: Protocol = Protocol::Ws2812;
    pub const DEFAULT_PIXEL_COUNT: u32 = super::PANEL_PIXELS;
    /// Vestigial too: the strip SPI is not wired on a panel board.
    pub const DEFAULT_DATA_PIN: u8 = 11;
    /// The 14 HUB75 panel pins (see `hub75_pins!`).
    pub const RESERVED_PINS: &[u8] = &[5, 4, 6, 15, 7, 17, 8, 18, 10, 9, 16, 12, 11, 13];
}

// UNTESTED ON METAL: no C6 on the bench. Wiring is reviewed against the
// ESP32-C6-DevKitC-1 pinout (SPI2/FSPI IO_MUX pins), never lit up.
#[cfg(feature = "board-c6-devkit")]
mod def {
    use super::*;
    pub const NAME: &str = "ESP32-C6 devkit (untested)";
    pub const DEFAULT_PROTOCOL: Protocol = Protocol::Ws2812;
    pub const DEFAULT_PIXEL_COUNT: u32 = 60;
    pub const DEFAULT_DATA_PIN: u8 = 7;
    /// SPI CLK (6), onboard RGB LED (8).
    pub const RESERVED_PINS: &[u8] = &[6, 8];
}

/// Hard cap on a runtime pixel count, per board — it bounds heap use
/// (engine frame buffer, crossfade blend buffer, outpipe buffer, the SPI
/// encode buffer) and is what `/api/config` validates against, what
/// `/api/status` reports as `max_pixels`, and what the playground's pixel
/// control clamps to. Gitea #74.
///
/// A 64x64 HUB75 panel is 4096 pixels, so panel boards must allow that;
/// strip boards stay at 2048. The split is deliberate rather than a global
/// raise: on the classic ESP32 a 4096-px WS2812 encode buffer alone is
/// ~36 KB, which the 80 KB heap can't carry alongside the WiFi blob. The
/// panel path never builds that buffer (the HUB75 driver owns two
/// bitplane framebuffers instead, allocated once at boot).
#[cfg(feature = "hub75")]
pub const MAX_PIXELS: u32 = 4096;
#[cfg(not(feature = "hub75"))]
pub const MAX_PIXELS: u32 = 2048;

/// Panel area = the default (and maximum useful) pixel count on a matrix
/// board. Geometry is compile-time (see hub75.rs) so this is a const.
#[cfg(feature = "hub75")]
pub const PANEL_PIXELS: u32 = (crate::hub75::PANEL_COLS * crate::hub75::PANEL_ROWS) as u32;

// The whole panel must be addressable, or the bottom rows render black —
// exactly the cap-clamped half panel that shipped before #74.
#[cfg(feature = "hub75")]
const _: () = assert!(PANEL_PIXELS <= MAX_PIXELS);

/// HUB75 pin map, per board. Pins are esp-hal *types*, not data, so this
/// is a macro rather than a const table — but it lives here with the rest
/// of the board identity, so a second panel board is a def-block diff and
/// main.rs keeps one wiring line. Expands to an `esp_hub75::Hub75Pins16`,
/// consuming the peripherals it names.
#[cfg(feature = "hub75")]
macro_rules! hub75_pins {
    ($p:ident) => {{
        use esp_hal::gpio::Pin as _;
        // ESP32-S3-DevKitC-1 with a panel on jumper wires: the esp-hub75 S3
        // example's map (clear of octal-PSRAM GPIO33-37; GPIO46 is
        // input-strapping at reset, safe as an address output after boot).
        #[cfg(feature = "board-s3-devkit")]
        let pins = esp_hub75::Hub75Pins16 {
            red1: $p.GPIO38.degrade(),
            grn1: $p.GPIO42.degrade(),
            blu1: $p.GPIO48.degrade(),
            red2: $p.GPIO47.degrade(),
            grn2: $p.GPIO2.degrade(),
            blu2: $p.GPIO21.degrade(),
            addr0: $p.GPIO14.degrade(),
            addr1: $p.GPIO46.degrade(),
            addr2: $p.GPIO13.degrade(),
            addr3: $p.GPIO9.degrade(),
            addr4: $p.GPIO3.degrade(),
            blank: $p.GPIO11.degrade(),
            clock: $p.GPIO12.degrade(),
            latch: $p.GPIO10.degrade(),
        };
        // Seengreat RGB Matrix HUB75 S3, from the vendor wiki's GPIO table
        // (seengreat.com/wiki/214). Both HUB75 outputs (ribbon connector
        // and the direct plug-in header) are wired to the same pins, so one
        // map drives either. The GPIO numbers are in no useful order (the
        // wiki lays them out two signals per row), so transcribe by signal
        // NAME — a positional read gives a colour-swapped panel.
        // Untouched by Luxel: audio (IO3/14/21/38/47/48),
        // microSD (IO39-42), I2C (IO1/IO2).
        #[cfg(feature = "board-seengreat-hub75")]
        let pins = esp_hub75::Hub75Pins16 {
            red1: $p.GPIO5.degrade(),
            grn1: $p.GPIO4.degrade(),
            blu1: $p.GPIO6.degrade(),
            red2: $p.GPIO15.degrade(),
            grn2: $p.GPIO7.degrade(),
            blu2: $p.GPIO17.degrade(),
            addr0: $p.GPIO8.degrade(),  // A
            addr1: $p.GPIO18.degrade(), // B
            addr2: $p.GPIO10.degrade(), // C
            addr3: $p.GPIO9.degrade(),  // D
            addr4: $p.GPIO16.degrade(), // E — 64 rows need it
            blank: $p.GPIO13.degrade(), // OE
            clock: $p.GPIO12.degrade(),
            latch: $p.GPIO11.degrade(), // LAT
        };
        pins
    }};
}
#[cfg(feature = "hub75")]
pub(crate) use hub75_pins;

// The HUB75 driver is LCD_CAM code — only the S3 has the peripheral
// (C3/S2 have no parallel output at all; classic-ESP32/C6 would need the
// I2S/PARLIO paths of esp-hub75, not wired up here).
#[cfg(all(feature = "hub75", not(feature = "esp32s3")))]
compile_error!("feature `hub75` requires an ESP32-S3 board (LCD_CAM)");

#[cfg(not(any(
    feature = "board-c3-devkit",
    feature = "board-pixelblaze-v3",
    feature = "board-athom-music",
    feature = "board-esp32-generic",
    feature = "board-s3-devkit",
    feature = "board-c6-devkit",
    feature = "board-seengreat-hub75",
)))]
compile_error!(
    "no board selected — build with --features board-<name> \
     (see docs/boards.md; e.g. board-pixelblaze-v3, board-esp32-generic)"
);

#[cfg(any(
    feature = "board-c3-devkit",
    feature = "board-pixelblaze-v3",
    feature = "board-athom-music",
    feature = "board-esp32-generic",
    feature = "board-s3-devkit",
    feature = "board-c6-devkit",
    feature = "board-seengreat-hub75",
))]
pub use def::*;

// ---- Runtime pin tables (Gitea #154 data-pin picker, #177 pattern GPIO) ----
//
// Pin NUMBERS are data even though pins are types: `esp_hal::gpio::AnyPin::
// steal(n)` erases the type at runtime, and these tables say which numbers
// are safe to hand it. Three layers, all `const`:
//
// - `chip`: what the silicon has — which GPIO numbers exist, which are
//   input-only, which carry the SPI flash / PSRAM (touching those hangs the
//   chip), which are the USB-serial / UART0 console, which reach ADC1.
// - `def::RESERVED_PINS` (per board, above): what Luxel itself drives —
//   the strip CLK, a relay, a status LED, the HUB75 bus.
// - the configured strip DATA pin (`shared::DATA_PIN`), reserved at runtime
//   by `gpio::pin_is_free`.
//
// A pattern naming a pin outside the free set is ignored on that pin
// (logged once); the data-pin picker refuses such a pin outright.

#[cfg(feature = "esp32")]
mod chip {
    /// Classic ESP32: GPIO 20, 24, 28–31 do not exist; 34–39 are input-only.
    pub const fn gpio_exists(n: u8) -> bool {
        matches!(n, 0..=5 | 12..=19 | 21..=23 | 25..=27 | 32..=39)
    }
    pub const fn gpio_can_output(n: u8) -> bool {
        gpio_exists(n) && n < 34
    }
    /// SPI flash (6–11), UART0 console (1, 3).
    pub const SYSTEM_PINS: &[u8] = &[6, 7, 8, 9, 10, 11, 1, 3];
    /// ADC1 channels (ADC2 is unusable while WiFi runs).
    pub const ADC1_PINS: &[u8] = &[32, 33, 34, 35, 36, 37, 38, 39];
}

#[cfg(feature = "esp32c3")]
mod chip {
    pub const fn gpio_exists(n: u8) -> bool {
        n <= 21
    }
    pub const fn gpio_can_output(n: u8) -> bool {
        gpio_exists(n)
    }
    /// SPI flash (11–17), USB-serial-JTAG (18, 19), UART0 (20, 21).
    pub const SYSTEM_PINS: &[u8] = &[11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21];
    pub const ADC1_PINS: &[u8] = &[0, 1, 2, 3, 4];
}

#[cfg(feature = "esp32s3")]
mod chip {
    /// GPIO 22–25 do not exist.
    pub const fn gpio_exists(n: u8) -> bool {
        matches!(n, 0..=21 | 26..=48)
    }
    pub const fn gpio_can_output(n: u8) -> bool {
        gpio_exists(n)
    }
    /// SPI flash (26–32), octal PSRAM (33–37), USB (19, 20), UART0 (43, 44).
    pub const SYSTEM_PINS: &[u8] = &[26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 19, 20, 43, 44];
    pub const ADC1_PINS: &[u8] = &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
}

#[cfg(feature = "esp32c6")]
mod chip {
    pub const fn gpio_exists(n: u8) -> bool {
        n <= 30
    }
    pub const fn gpio_can_output(n: u8) -> bool {
        gpio_exists(n)
    }
    /// SPI flash (24–30), USB-serial-JTAG (12, 13), UART0 (16, 17).
    pub const SYSTEM_PINS: &[u8] = &[24, 25, 26, 27, 28, 29, 30, 12, 13, 16, 17];
    /// esp-hal 1.1 carries no ADC channel map for the C6, so `analogRead`
    /// stays at 0 there (gpio.rs logs it once).
    pub const ADC1_PINS: &[u8] = &[];
}

pub use chip::*;

const fn in_list(list: &[u8], n: u8) -> bool {
    let mut i = 0;
    while i < list.len() {
        if list[i] == n {
            return true;
        }
        i += 1;
    }
    false
}

/// A GPIO number that exists on this chip and is neither a system pin
/// (flash, PSRAM, console) nor one the board itself drives. Does NOT
/// account for the runtime strip DATA pin — `gpio::pin_is_free` does.
pub const fn pin_is_board_free(n: u8) -> bool {
    gpio_exists(n) && !in_list(SYSTEM_PINS, n) && !in_list(RESERVED_PINS, n)
}

/// Whether `n` may carry the strip DATA line: board-free AND able to
/// drive an output (the classic ESP32's 34–39 cannot).
pub const fn data_pin_ok(n: u8) -> bool {
    pin_is_board_free(n) && gpio_can_output(n)
}

/// Whether `n` reaches ADC1 on this chip (still subject to `pin_is_free`).
pub const fn adc_pin(n: u8) -> bool {
    in_list(ADC1_PINS, n)
}

// Every board's default DATA pin must pass its own picker check — a board
// whose default is "reserved" would boot with the strip dark and no way
// to select it back.
const _: () = assert!(data_pin_ok(DEFAULT_DATA_PIN) || cfg!(feature = "hub75"));
