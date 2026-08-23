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
// default pixel count becomes the panel area (clamped to the runtime cap
// — 2048 until the per-board cap of Gitea #74 lands, so the lower half
// of a 64x64 panel stays dark) and DEFAULT_PROTOCOL is vestigial (the
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
    pub const DEFAULT_PIXEL_COUNT: u32 = {
        let area = (crate::hub75::PANEL_COLS * crate::hub75::PANEL_ROWS) as u32;
        if area > crate::shared::MAX_PIXELS { crate::shared::MAX_PIXELS } else { area }
    };
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
))]
pub use def::*;
