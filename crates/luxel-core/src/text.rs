// STUB (#484 replaces this file wholesale)
//
//! Text rendering seam — the signatures `scene.rs` and `compose.rs` compile
//! against while C2 (Gitea #484) builds the real PSF2 font machinery.
//!
//! Everything here is inert: [`draw`] paints nothing and returns a zero
//! advance, [`width`] measures zero, [`with_slot`] hands out the empty
//! string. A scene with a text layer therefore composites as if the layer
//! were empty rather than failing — which is exactly what a device whose
//! firmware predates the fonts should do.
//!
//! The contract CORE-B must preserve (webui-v2 Phase C contract §5/§9):
//! `draw`, `width`, `Font`, `ClockFmt`, `format_clock`, `set_slot`,
//! `with_slot`. The compositor itself only uses `draw`, `width` and `Font`;
//! clock and slot text are resolved by the HOST and pushed in through
//! `Compositor::set_text`, so the rest of this file is host surface.

use alloc::string::String;

use crate::outpipe::GridMap;

/// One of the three built-in bitmap fonts (C2 ships the blobs).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Font {
    /// 4x6 cell.
    Tiny,
    /// 5x7 cell.
    #[default]
    Regular,
    /// 5x8 cell.
    Large,
}

impl Font {
    /// Wire/JSON spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Font::Tiny => "tiny",
            Font::Regular => "regular",
            Font::Large => "large",
        }
    }

    /// Parse the wire spelling.
    pub fn from_wire(s: &str) -> Option<Font> {
        match s {
            "tiny" => Some(Font::Tiny),
            "regular" => Some(Font::Regular),
            "large" => Some(Font::Large),
            _ => None,
        }
    }
}

/// The clock renderings a text layer (or `/api/text`) can ask for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClockFmt {
    /// `HH:MM` — 24 hour.
    Hm24,
    /// `HH:MM:SS` — 24 hour.
    Hms24,
    /// `hh:MM` — 12 hour, no leading zero.
    Hm12,
    /// `hh:MM:SS` — 12 hour, no leading zero.
    Hms12,
    /// `MM-DD`.
    MonthDay,
    /// `YYYY-MM-DD`.
    Date,
}

impl ClockFmt {
    /// Wire/JSON spelling — the literal the format is named by.
    pub const fn as_str(self) -> &'static str {
        match self {
            ClockFmt::Hm24 => "HH:MM",
            ClockFmt::Hms24 => "HH:MM:SS",
            ClockFmt::Hm12 => "hh:MM",
            ClockFmt::Hms12 => "hh:MM:SS",
            ClockFmt::MonthDay => "MM-DD",
            ClockFmt::Date => "YYYY-MM-DD",
        }
    }

    pub fn from_wire(s: &str) -> Option<ClockFmt> {
        match s {
            "HH:MM" => Some(ClockFmt::Hm24),
            "HH:MM:SS" => Some(ClockFmt::Hms24),
            "hh:MM" => Some(ClockFmt::Hm12),
            "hh:MM:SS" => Some(ClockFmt::Hms12),
            "MM-DD" => Some(ClockFmt::MonthDay),
            "YYYY-MM-DD" => Some(ClockFmt::Date),
            _ => None,
        }
    }
}

/// Number of host-installable text slots (`/api/text`, `textSlot(n)`).
pub const SLOTS: usize = 8;

/// Longest text a slot holds, in UTF-8 bytes.
pub const SLOT_MAX: usize = 64;

/// Draw `text` with its top-left at grid cell (`x`, `y`), returning the
/// advance in pixels. Clips to the grid; a no-op without one, like every
/// `bulk.rs` kernel. STUB: draws nothing, advance 0.
pub fn draw(
    _dst: &mut [[u8; 3]],
    _grid: &GridMap,
    _x: i32,
    _y: i32,
    _text: &str,
    _font: Font,
    _rgb: [u8; 3],
) -> i32 {
    0
}

/// Rendered width of `text` in pixels. STUB: 0.
pub fn width(_text: &str, _font: Font) -> u32 {
    0
}

/// Render a wall-clock reading in `fmt`. STUB: the empty string.
#[allow(clippy::too_many_arguments)]
pub fn format_clock(
    _fmt: ClockFmt,
    _h: u8,
    _m: u8,
    _s: u8,
    _y: u16,
    _mo: u8,
    _d: u8,
) -> String {
    String::new()
}

/// Install slot `n`'s text (truncated to [`SLOT_MAX`] on a char boundary).
/// STUB: discards it.
pub fn set_slot(_n: u8, _s: &str) {}

/// Read slot `n` under the table's lock. STUB: always the empty string.
pub fn with_slot<R>(_n: u8, f: impl FnOnce(&str) -> R) -> R {
    f("")
}
