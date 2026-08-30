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
}

#[cfg(feature = "board-pixelblaze-v3")]
mod def {
    use super::*;
    pub const NAME: &str = "Pixelblaze v3 Standard";
    pub const DEFAULT_PROTOCOL: Protocol = Protocol::Sk9822;
    pub const DEFAULT_PIXEL_COUNT: u32 = 300;
}

#[cfg(feature = "board-athom-music")]
mod def {
    use super::*;
    pub const NAME: &str = "Athom music-reactive WLED controller";
    pub const DEFAULT_PROTOCOL: Protocol = Protocol::Ws2812;
    pub const DEFAULT_PIXEL_COUNT: u32 = 60;
}

#[cfg(feature = "board-esp32-generic")]
mod def {
    use super::*;
    pub const NAME: &str = "generic ESP32 (VSPI: CLK 18, DATA 23)";
    pub const DEFAULT_PROTOCOL: Protocol = Protocol::Ws2812;
    pub const DEFAULT_PIXEL_COUNT: u32 = 60;
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
}

// UNTESTED ON METAL: no C6 on the bench. Wiring is reviewed against the
// ESP32-C6-DevKitC-1 pinout (SPI2/FSPI IO_MUX pins), never lit up.
#[cfg(feature = "board-c6-devkit")]
mod def {
    use super::*;
    pub const NAME: &str = "ESP32-C6 devkit (untested)";
    pub const DEFAULT_PROTOCOL: Protocol = Protocol::Ws2812;
    pub const DEFAULT_PIXEL_COUNT: u32 = 60;
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
