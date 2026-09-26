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
    /// CLK1 (5), CLK2 (16), strip-power relay (2). The case button (0), IR
    /// receiver (25) and mic pins (32/15/36) are NOT reserved: Luxel leaves
    /// them idle, so a pattern may read them (`digitalRead(0)` is the
    /// button). CLK2 joined the list with #474: the second output's SPI
    /// binds it as its clock, so it is no longer a pad a strip or a pattern
    /// may take.
    pub const RESERVED_PINS: &[u8] = &[5, 2, 16];

    /// The second output's SPI clock pad (CLK2), on the boards that have a
    /// second output at all — [`super::OUTPUTS`] `> 1`. Its DATA pad is not
    /// a constant: it is whatever `out 1` in the Layout names (GPIO17 as the
    /// board wires DATA2), exactly like output 0's.
    pub const SECOND_CLK_PIN: u8 = 16;
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
    /// The 14 HUB75 signals, in `hub75_pins!`'s own order (see
    /// [`super::HUB75_PINS`]).
    #[cfg(feature = "hub75")]
    pub const HUB75_PINS: [u8; 14] = [38, 42, 48, 47, 2, 21, 14, 46, 13, 9, 3, 11, 12, 10];
    /// SPI CLK (12); with `hub75`, the 14 panel pins from `hub75_pins!`.
    #[cfg(not(feature = "hub75"))]
    pub const RESERVED_PINS: &[u8] = &[12];
    #[cfg(feature = "hub75")]
    pub const RESERVED_PINS: &[u8] = &HUB75_PINS;
}

// On metal since 2026-09-05 (Gitea #75; docs/boards.md "First light").
// Seengreat "RGB Matrix HUB75 S3" panel driver board: ESP32-S3-WROOM-1
// (16 MB flash / 8 MB octal PSRAM) with two HUB75 outputs (ribbon +
// direct plug-in header), an ES7210/ES8311 codec, microSD and an RTC.
// Luxel uses the HUB75 side only — the codec, SD and RTC are unwired
// (sound-reactive work on this board is Gitea #142). The PSRAM IS
// initialised here (`psram-arena`, src/psram.rs): it is a second,
// separate esp-alloc heap used only for pattern-array storage, so the
// DMA framebuffers and every hot per-frame buffer stay in internal SRAM
// (Gitea #253, docs/boards.md).
#[cfg(feature = "board-seengreat-hub75")]
mod def {
    use super::*;
    pub const NAME: &str = "Seengreat RGB Matrix HUB75 S3";
    // Vestigial: the panel driver's wire format is fixed and
    // set_protocol() rejects switches. Kept because the field is part of
    // the persisted device config on every board.
    pub const DEFAULT_PROTOCOL: Protocol = Protocol::Ws2812;
    pub const DEFAULT_PIXEL_COUNT: u32 = super::PANEL_PIXELS;
    /// Vestigial too: the strip SPI is not wired on a panel board.
    pub const DEFAULT_DATA_PIN: u8 = 11;
    /// The 14 HUB75 signals, in `hub75_pins!`'s own order (see
    /// [`super::HUB75_PINS`]).
    pub const HUB75_PINS: [u8; 14] = [5, 4, 6, 15, 7, 17, 8, 18, 10, 9, 16, 12, 11, 13];
    /// The 14 HUB75 panel pins (see `hub75_pins!`).
    pub const RESERVED_PINS: &[u8] = &HUB75_PINS;
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

/// Physical LED outputs this BOARD has, whatever the firmware drives today
/// — `/api/status`'s `caps.outputs`, which is what makes the Settings page
/// show an Outputs table instead of inline strip fields (proposal §5.3b).
/// Since Gitea #474 the firmware drives every output the Layout configures,
/// up to this many — so this describes the hardware, and the `out` table
/// describes the current wiring.
///
/// The Athom WLED controller has two strip channels (DATA1/CLK1 and
/// DATA2/CLK2 — docs/boards.md), and since #474 the firmware drives both:
/// each output takes a consecutive run of the one pixel space. The
/// Seengreat panel board's two HUB75 connectors are the SAME pins wired
/// twice (board::hub75_pins!), so they are one output, not two. Every other
/// board here breaks out one.
#[cfg(feature = "board-athom-music")]
pub const OUTPUTS: u8 = 2;
#[cfg(not(feature = "board-athom-music"))]
pub const OUTPUTS: u8 = 1;

// The second driver instance is a build.rs cfg (so a one-output board's
// image is byte-identical to before #474) and OUTPUTS is the same fact as
// data. They must never disagree: a board advertising two outputs with no
// second driver would accept an `out 1` line and silently drive nothing.
const _: () = assert!((OUTPUTS > 1) == cfg!(multi_output));
// …and the pad that driver clocks on must be one nothing else can take.
#[cfg(multi_output)]
const _: () = assert!(in_list(RESERVED_PINS, SECOND_CLK_PIN));

/// Whether the DEVICE output chain's blur and glow stages fit this board's
/// per-frame budget — `/api/status`'s `caps.blur_glow`, which is what makes
/// the Settings page offer them at all (proposal D12, Gitea #476).
///
/// False on a HUB75 panel. At 4096 px the two spatial stages run over a
/// 64x64 grid — four separable passes plus a neighbour max — and the compose
/// window on the S3 is one panel rescan. Measured on the Seengreat panel
/// 2026-09-19 (`/api/status` `pipe_us` against `pass.nominal_us`): 49 us
/// idle, 4,508 us with blur at 50 %, **8,780 us with blur+glow at 50/50
/// against a 8,665 us rescan**. The compose alone overruns the window, so
/// every frame arrives a rescan late and the panel shows the previous one —
/// the board does not drop frames, it halves its refresh. A strip has no
/// such window (2,443 us at 2048 px on the Athom, against no clock at all)
/// and keeps both.
///
/// The PATTERN-side `setBlur`/`setGlow` are a different chain
/// (`Engine::post_chain`) and are unaffected — this flag hides an
/// installation-wide setting, not a pattern's own look.
#[cfg(feature = "hub75")]
pub const BLUR_GLOW: bool = false;
#[cfg(not(feature = "hub75"))]
pub const BLUR_GLOW: bool = true;

/// Area of the DEFAULT panel = the default pixel count on a matrix board.
/// Panel geometry itself is a runtime setting since #401 (`hub75.rs`, the
/// `matrix` line); this is only the board's own default, which is what the
/// device record comes up with when nothing is stored.
#[cfg(feature = "hub75")]
pub const PANEL_PIXELS: u32 =
    crate::hub75::DEFAULT_PANEL_W as u32 * crate::hub75::DEFAULT_PANEL_H as u32;

// The whole DEFAULT panel must be addressable, or the bottom rows render
// black — exactly the cap-clamped half panel that shipped before #74. A
// CONFIGURED panel larger than the cap is refused at boot (`hub75::try_boot`)
// and by the layout parser's own `max_pixels` check.
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

/// The same 14 pins as [`hub75_pins!`], built from their NUMBERS instead of
/// the typed peripherals — `HUB75_PINS` per board, in the macro's own field
/// order (red1 grn1 blu1 red2 grn2 blu2 addr0..4 blank clock latch).
///
/// Needed because `Hub75::new` CONSUMES the pins: when it fails at the
/// configured settings the fallback attempt (`hub75::Hub75Output::new`, #401)
/// has no `Hub75Pins16` left to hand a second driver, and the typed
/// peripherals are long gone from `main`.
///
/// # Safety
/// Only one `Hub75Pins16` may exist at a time. Every pad here is in
/// `RESERVED_PINS` — which IS `HUB75_PINS` — so `data_pin_ok` and
/// `gpio::pin_is_free` exclude all of them and nothing else in the firmware
/// can name one; the caller must only ensure the previous set is gone.
#[cfg(feature = "hub75")]
pub(crate) unsafe fn hub75_pins_stolen() -> esp_hub75::Hub75Pins16<'static> {
    let p = |i: usize| unsafe { esp_hal::gpio::AnyPin::steal(HUB75_PINS[i]) };
    esp_hub75::Hub75Pins16 {
        red1: p(0),
        grn1: p(1),
        blu1: p(2),
        red2: p(3),
        grn2: p(4),
        blu2: p(5),
        addr0: p(6),
        addr1: p(7),
        addr2: p(8),
        addr3: p(9),
        addr4: p(10),
        blank: p(11),
        clock: p(12),
        latch: p(13),
    }
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
