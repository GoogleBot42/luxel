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

#[cfg(not(any(
    feature = "board-c3-devkit",
    feature = "board-pixelblaze-v3",
    feature = "board-athom-music",
    feature = "board-esp32-generic",
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
))]
pub use def::*;
