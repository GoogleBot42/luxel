//! Text: the built-in PSF2 fonts, the draw/measure kernel, number and clock
//! formatting, and the host-installable text-slot table.
//!
//! One implementation, three hosts: the `drawText` / `textWidth` /
//! `drawNumber` / `font` / `textSlot` builtins (`crate::vm`), the scene
//! compositor's text layer (`crate::compose`) and the playground all call
//! into here, so a panel shows the same pixels whoever drew them.
//!
//! ## Fonts
//!
//! Three faces are `include_bytes!`d from `crates/luxel-core/fonts/` as
//! **PSF2** — a 32-byte little-endian header then `length × charsize` bytes
//! of MSB-first row-packed glyphs, which is O(1) lookup and zero parsing
//! state. Provenance, licences and the exact conversion command are in
//! `crates/luxel-core/fonts/README.md`; they are built by
//! `tools/fonts/bdf2psf2.py`.
//!
//! Every blob holds exactly the 95 printable ASCII glyphs `0x20..=0x7E` in
//! order, so glyph index = `codepoint - 0x20` and there is no Unicode
//! table. Any other code point draws `?`.
//!
//! | name | cell | advance | bytes |
//! |---|---|---|---|
//! | `tiny` | 3×6 (Tom Thumb) | 4 | 602 |
//! | `regular` | 5×7 (X11 misc-fixed) | 6 | 697 |
//! | `large` | 5×8 (Spleen) | 6 | 792 |
//!
//! The advance is `cell width + 1`: the inter-glyph gap is not baked into
//! the cell, which is what makes the tiny face exactly 4 px per character.
//!
//! ## Drawing
//!
//! Grid space, **top-left origin, y growing down**, the current brush
//! colour, and — like every kernel in `crate::bulk` — a silent no-op when
//! the frame has no regular grid behind it.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::fmt;
use core::ops::Deref;

use crate::fixed::Fx;
use crate::outpipe::GridMap;

// ------------------------------------------------------------ fixed strings

/// A `String` with its capacity in the type and its bytes on the stack —
/// what the formatters below write into on a `no_std` device with no
/// allocator in reach. Always valid UTF-8: the only ways in are `push_str`
/// and `push`, and both refuse a partial write rather than splitting a
/// character.
#[derive(Clone, Copy)]
pub struct TextBuf<const N: usize> {
    buf: [u8; N],
    len: usize,
}

/// The formatters' buffer: `YYYY-MM-DD` and a 10-digit, 4-decimal signed
/// number both fit with room to spare.
pub type TextString = TextBuf<24>;

impl<const N: usize> Default for TextBuf<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> TextBuf<N> {
    pub const fn new() -> Self {
        TextBuf {
            buf: [0; N],
            len: 0,
        }
    }

    pub fn as_str(&self) -> &str {
        // SAFETY: only `push_str`/`push` write, and both copy whole
        // `&str`s, so the prefix is always valid UTF-8.
        unsafe { core::str::from_utf8_unchecked(&self.buf[..self.len]) }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Append `s`, or nothing at all when it would not fit.
    pub fn push_str(&mut self, s: &str) -> bool {
        let b = s.as_bytes();
        if self.len + b.len() > N {
            return false;
        }
        self.buf[self.len..self.len + b.len()].copy_from_slice(b);
        self.len += b.len();
        true
    }

    /// Append one ASCII byte. Non-ASCII is rejected — nothing here needs it
    /// and it is what keeps the buffer valid UTF-8 without a re-scan.
    pub fn push_ascii(&mut self, c: u8) -> bool {
        if c > 0x7F || self.len == N {
            return false;
        }
        self.buf[self.len] = c;
        self.len += 1;
        true
    }
}

impl<const N: usize> Deref for TextBuf<N> {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl<const N: usize> fmt::Debug for TextBuf<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl<const N: usize> fmt::Display for TextBuf<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<const N: usize> PartialEq<str> for TextBuf<N> {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

// ------------------------------------------------------------------- fonts

/// The built-in faces. Referenced by NAME everywhere a user can pick one
/// (`font("tiny")` in a pattern, `F tiny …` in a scene record).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Font {
    /// Tom Thumb, 3×6 cell, 4 px advance — ~16 characters across a 64-wide
    /// panel.
    Tiny,
    /// X11 misc-fixed 5×7, 6 px advance. The readability sweet spot.
    #[default]
    Regular,
    /// Spleen 5×8, 6 px advance.
    Large,
}

static TINY: &[u8] = include_bytes!("../fonts/tiny.psf");
static REGULAR: &[u8] = include_bytes!("../fonts/regular.psf");
static LARGE: &[u8] = include_bytes!("../fonts/large.psf");

/// Every face, in wire order — the list a font picker shows.
pub const FONTS: [Font; 3] = [Font::Tiny, Font::Regular, Font::Large];

impl Font {
    /// The wire/pattern name (`tiny` | `regular` | `large`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Font::Tiny => "tiny",
            Font::Regular => "regular",
            Font::Large => "large",
        }
    }

    /// A face by name; `None` for anything else — `font()` leaves the
    /// current face alone rather than erroring, and a scene record rejects
    /// the line.
    pub fn from_wire(s: &str) -> Option<Font> {
        match s {
            "tiny" => Some(Font::Tiny),
            "regular" => Some(Font::Regular),
            "large" => Some(Font::Large),
            _ => None,
        }
    }

    fn blob(self) -> &'static [u8] {
        match self {
            Font::Tiny => TINY,
            Font::Regular => REGULAR,
            Font::Large => LARGE,
        }
    }

    /// Cell width in pixels (the ink box — the advance is one more).
    pub fn cell_width(self) -> u32 {
        self.psf().width
    }

    /// Cell height in pixels.
    pub fn height(self) -> u32 {
        self.psf().height
    }

    /// Pixels from one glyph's left edge to the next's.
    pub fn advance(self) -> u32 {
        self.cell_width() + 1
    }

    fn psf(self) -> Psf2 {
        // Every blob ships with this crate and is checked by
        // `fonts_are_well_formed` below, so a malformed one is a build
        // error, not a runtime branch.
        Psf2::parse(self.blob())
    }
}

/// The parsed PSF2 header plus the glyph sheet it points at.
#[derive(Clone, Copy)]
struct Psf2 {
    width: u32,
    height: u32,
    /// Bytes per glyph = `ceil(width / 8) × height`.
    charsize: u32,
    /// Glyphs present, starting at `FIRST_CP`.
    length: u32,
    glyphs: &'static [u8],
}

/// First code point in every blob. Glyph index is `cp - FIRST_CP`.
const FIRST_CP: u32 = 0x20;
/// Last code point in every blob.
const LAST_CP: u32 = 0x7E;
/// PSF2's magic. The blobs ship with the crate and `fonts_are_well_formed_psf2`
/// checks them, so `parse` below does not re-check it at runtime.
#[cfg(test)]
const PSF2_MAGIC: [u8; 4] = [0x72, 0xB5, 0x4A, 0x86];

impl Psf2 {
    fn parse(blob: &'static [u8]) -> Psf2 {
        let u32_at = |o: usize| {
            u32::from_le_bytes([blob[o], blob[o + 1], blob[o + 2], blob[o + 3]])
        };
        let headersize = u32_at(8) as usize;
        Psf2 {
            length: u32_at(16),
            charsize: u32_at(20),
            height: u32_at(24),
            width: u32_at(28),
            glyphs: &blob[headersize..],
        }
    }

    /// The glyph for `cp`, falling back to `?` outside the sheet.
    fn glyph(&self, cp: u32) -> &'static [u8] {
        let i = if (FIRST_CP..=LAST_CP).contains(&cp) {
            cp - FIRST_CP
        } else {
            b'?' as u32 - FIRST_CP
        };
        // the sheet is 95 glyphs by construction (asserted in the tests);
        // this keeps a hand-edited blob from indexing past its body
        let i = if i < self.length { i } else { 0 };
        let off = (i * self.charsize) as usize;
        &self.glyphs[off..off + self.charsize as usize]
    }

    /// Is the pixel at (`col`, `row`) of `glyph` ink?
    #[inline]
    fn ink(&self, glyph: &[u8], row: u32, col: u32) -> bool {
        let stride = self.width.div_ceil(8);
        let byte = glyph[(row * stride + col / 8) as usize];
        byte >> (7 - (col & 7)) & 1 == 1
    }
}

// ----------------------------------------------------------------- drawing

/// Where `x` sits relative to the run of text.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Align {
    /// `x` is the left edge (the `drawText` default).
    #[default]
    Left,
    /// `x` is the centre.
    Center,
    /// `x` is the right edge.
    Right,
}

impl Align {
    /// The `drawText` argument: 0 left, 1 centre, 2 right; anything else
    /// reads as left, matching the "missing args read as 0" convention.
    pub fn from_code(v: i32) -> Align {
        match v {
            1 => Align::Center,
            2 => Align::Right,
            _ => Align::Left,
        }
    }

    /// The scene-record letter (`l` | `c` | `r`).
    pub fn letter(self) -> &'static str {
        match self {
            Align::Left => "l",
            Align::Center => "c",
            Align::Right => "r",
        }
    }

    pub fn from_letter(s: &str) -> Option<Align> {
        match s {
            "l" => Some(Align::Left),
            "c" => Some(Align::Center),
            "r" => Some(Align::Right),
            _ => None,
        }
    }

    /// The left edge of a `w`-wide run anchored at `x`.
    fn origin(self, x: i32, w: u32) -> i32 {
        match self {
            Align::Left => x,
            Align::Center => x - (w as i32) / 2,
            Align::Right => x - w as i32,
        }
    }
}

/// Advance width of `text` in `font`, in pixels — `n × (cell + 1)`,
/// trailing gap included, so laying two runs side by side is one add.
pub fn width(text: &str, font: Font) -> u32 {
    text.chars().count() as u32 * font.advance()
}

/// Draw `text` into the frame at grid cell (`x`, `y`) — **top-left origin,
/// y growing down** — in `rgb`, and return its advance width.
///
/// Clips at all four edges; a glyph entirely off the grid costs nothing but
/// its column walk. Cells past the end of `dst` (the tail of an
/// over-provisioned last row) address no pixel and are skipped, exactly as
/// in `bulk::paste`.
pub fn draw(
    dst: &mut [[u8; 3]],
    grid: &GridMap,
    x: i32,
    y: i32,
    text: &str,
    font: Font,
    rgb: [u8; 3],
) -> i32 {
    draw_aligned(dst, grid, x, y, text, font, rgb, Align::Left)
}

/// [`draw`] with an explicit anchor.
#[allow(clippy::too_many_arguments)]
pub fn draw_aligned(
    dst: &mut [[u8; 3]],
    grid: &GridMap,
    x: i32,
    y: i32,
    text: &str,
    font: Font,
    rgb: [u8; 3],
    align: Align,
) -> i32 {
    let w = width(text, font);
    let psf = font.psf();
    let (gw, gh) = (grid.w as i32, grid.h as i32);
    let mut pen = align.origin(x, w);
    for ch in text.chars() {
        // whole glyph off the left/right edge: skip the row walk
        if pen + psf.width as i32 > 0 && pen < gw {
            let g = psf.glyph(ch as u32);
            for row in 0..psf.height as i32 {
                let py = y + row;
                if py < 0 || py >= gh {
                    continue;
                }
                for col in 0..psf.width as i32 {
                    let px = pen + col;
                    if px < 0 || px >= gw {
                        continue;
                    }
                    if psf.ink(g, row as u32, col as u32) {
                        let i = grid.index(py as usize, px as usize);
                        if let Some(p) = dst.get_mut(i) {
                            *p = rgb;
                        }
                    }
                }
            }
        }
        pen += font.advance() as i32;
    }
    w as i32
}

// -------------------------------------------------------------- formatting

/// Largest `decimals` [`format_number`] honours — `10^4` is the widest
/// power of ten that keeps the rounding below in 32-bit range.
pub const MAX_DECIMALS: u8 = 4;
/// Largest `digits` [`format_number`] honours (a 16.16 integer part is at
/// most 5 digits; the rest is padding).
pub const MAX_DIGITS: u8 = 10;

/// `drawNumber`'s formatter: `v` with at least `digits` integer digits
/// (zero-padded) and exactly `decimals` fraction digits, rounded
/// half-up, with a leading `-` when negative.
///
/// Fixed-point aware: the fraction comes from the 16.16 word, so
/// `format_number(0.5, 1, 2)` is `0.50` and not `0.49`.
pub fn format_number(v: Fx, digits: u8, decimals: u8, out: &mut TextString) {
    out.clear();
    let decimals = decimals.min(MAX_DECIMALS);
    let digits = digits.min(MAX_DIGITS).max(1);

    let raw = v.raw();
    let neg = raw < 0;
    // i64 so i32::MIN negates cleanly
    let mag = (raw as i64).unsigned_abs();
    let mut int = (mag >> 16) as u64;
    let frac = (mag & 0xFFFF) as u64;

    let scale: u64 = match decimals {
        0 => 1,
        1 => 10,
        2 => 100,
        3 => 1000,
        _ => 10_000,
    };
    // round half-up on the scaled fraction, carrying into the integer part
    let mut fr = (frac * scale + 0x8000) >> 16;
    if fr >= scale {
        fr -= scale;
        int += 1;
    }

    if neg && (int != 0 || fr != 0) {
        out.push_ascii(b'-');
    }
    push_padded(out, int, digits);
    if decimals > 0 {
        out.push_ascii(b'.');
        push_padded(out, fr, decimals);
    }
}

/// `v` in decimal, left-padded with `0` to at least `pad` digits.
fn push_padded(out: &mut TextString, v: u64, pad: u8) {
    let mut tmp = [0u8; 20];
    let mut n = 0;
    let mut v = v;
    loop {
        tmp[n] = b'0' + (v % 10) as u8;
        v /= 10;
        n += 1;
        if v == 0 {
            break;
        }
    }
    for _ in n..pad as usize {
        out.push_ascii(b'0');
    }
    while n > 0 {
        n -= 1;
        out.push_ascii(tmp[n]);
    }
}

/// The clock/date layouts a scene text layer can pick (contract §1; the
/// wire names are the strings themselves).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClockFmt {
    /// `HH:MM` — 24-hour, zero-padded.
    Hm24,
    /// `HH:MM:SS` — 24-hour, zero-padded.
    Hms24,
    /// `hh:MM` — 12-hour, 1..12, NOT zero-padded (the LED-clock
    /// convention; `9:05`, not `09:05`).
    Hm12,
    /// `hh:MM:SS` — 12-hour, 1..12, not zero-padded.
    Hms12,
    /// `MM-DD`.
    MonthDay,
    /// `YYYY-MM-DD`.
    Date,
}

/// Every layout, in wire order — the list a picker shows.
pub const CLOCK_FMTS: [ClockFmt; 6] = [
    ClockFmt::Hm24,
    ClockFmt::Hms24,
    ClockFmt::Hm12,
    ClockFmt::Hms12,
    ClockFmt::MonthDay,
    ClockFmt::Date,
];

impl ClockFmt {
    /// The wire name, which is the layout itself (`HH:MM`, `YYYY-MM-DD`, …).
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
        let mut i = 0;
        while i < CLOCK_FMTS.len() {
            if CLOCK_FMTS[i].as_str() == s {
                return Some(CLOCK_FMTS[i]);
            }
            i += 1;
        }
        None
    }
}

/// Render wall-clock fields through `fmt`. The caller supplies whatever it
/// has: 24-hour `h` 0..23, `m`/`s` 0..59, `y` a full year, `mo` 1..12,
/// `d` 1..31. Out-of-range fields are clamped, never rejected — a device
/// with no time sync draws `00:00` rather than nothing.
/// Returns an `alloc::String` (not a [`TextString`]) because the
/// compositor's clock text source hands it straight to
/// `Compositor::set_text`, which takes a `&str` it will own.
#[allow(clippy::too_many_arguments)]
pub fn format_clock(fmt: ClockFmt, h: u8, m: u8, s: u8, y: u16, mo: u8, d: u8) -> String {
    let mut out = TextString::new();
    let (h, m, s) = (h.min(23), m.min(59), s.min(59));
    let (mo, d) = (mo.clamp(1, 12), d.clamp(1, 31));
    match fmt {
        ClockFmt::Hm24 | ClockFmt::Hms24 | ClockFmt::Hm12 | ClockFmt::Hms12 => {
            match fmt {
                ClockFmt::Hm12 | ClockFmt::Hms12 => {
                    let h12 = match h % 12 {
                        0 => 12,
                        n => n,
                    };
                    push_padded(&mut out, h12 as u64, 1);
                }
                _ => push_padded(&mut out, h as u64, 2),
            }
            out.push_ascii(b':');
            push_padded(&mut out, m as u64, 2);
            if matches!(fmt, ClockFmt::Hms24 | ClockFmt::Hms12) {
                out.push_ascii(b':');
                push_padded(&mut out, s as u64, 2);
            }
        }
        ClockFmt::MonthDay => {
            push_padded(&mut out, mo as u64, 2);
            out.push_ascii(b'-');
            push_padded(&mut out, d as u64, 2);
        }
        ClockFmt::Date => {
            push_padded(&mut out, y as u64, 4);
            out.push_ascii(b'-');
            push_padded(&mut out, mo as u64, 2);
            out.push_ascii(b'-');
            push_padded(&mut out, d as u64, 2);
        }
    }
    String::from(out.as_str())
}

// ------------------------------------------------------------- text slots

/// Text slots a host can write (`POST /api/text`, MQTT, an HA text entity)
/// and a pattern or a scene text layer can read.
pub const SLOTS: usize = 8;
/// Bytes one slot holds. A longer write is truncated on a char boundary.
pub const SLOT_MAX: usize = 64;

struct Slot {
    len: usize,
    buf: [u8; SLOT_MAX],
}

/// The slot table.
///
/// **Allocated on the first write, never before.** 8 × 64 B of `.bss` is
/// 544 bytes of internal SRAM on every board whether or not anything ever
/// draws a word, and `tools/stack-check.sh` measures exactly that trade
/// (statics come out of the main task's stack headroom). A device that
/// never receives a `POST /api/text` pays one pointer.
///
/// ## Single-writer rule
///
/// `luxel-core` is `no_std` and does not depend on `critical-section`, so
/// this is the same idiom [`crate::arena`]'s hook uses: a `static` behind
/// an `UnsafeCell` with a documented contract instead of a lock.
///
/// * [`set_slot`] is a **control-path** call — one HTTP/MQTT handler, one
///   task. It must never run concurrently with a render, i.e. never
///   concurrently with [`with_slot`] or with another `set_slot`. It is
///   also the only thing that allocates here, so it must run after the
///   network stack is up (`wait_config_up`), like every other multi-byte
///   boot-time load.
/// * [`with_slot`] borrows the slot for the duration of its closure and
///   must not call `set_slot` from inside it.
///
/// On the device, both the API task and the frame loop live on the same
/// executor and a frame never yields mid-render, so the rule holds by
/// construction; the playground is single-threaded. **The mirror is not** —
/// `luxel serve` handles each connection on its own thread — so it queues
/// every write to its render thread (`Msg::TextSlot`) rather than calling
/// this from a handler, and keeps its own `Mutex`'d copy for read-back.
struct SlotTable(UnsafeCell<Option<Box<[Slot]>>>);

// SAFETY: see the single-writer rule above — the embedder guarantees the
// writes and the reads do not overlap.
unsafe impl Sync for SlotTable {}

static TABLE: SlotTable = SlotTable(UnsafeCell::new(None));

/// Write slot `n`, truncating to [`SLOT_MAX`] bytes **on a char boundary**
/// so the stored value is always valid UTF-8. `n >= SLOTS` is ignored.
/// An empty `s` clears the slot.
///
/// Allocates the table on the first non-empty write (see above), and does
/// nothing at all if that allocation fails — a device out of heap keeps
/// rendering, it does not error out of an API call.
pub fn set_slot(n: u8, s: &str) {
    if n as usize >= SLOTS {
        return;
    }
    let mut cut = s.len().min(SLOT_MAX);
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    // SAFETY: single-writer rule.
    let table = unsafe { &mut *TABLE.0.get() };
    if table.is_none() {
        if cut == 0 {
            return; // clearing a table that does not exist yet
        }
        let mut v: Vec<Slot> = Vec::new();
        if v.try_reserve_exact(SLOTS).is_err() {
            return;
        }
        v.resize_with(SLOTS, || Slot {
            len: 0,
            buf: [0; SLOT_MAX],
        });
        *table = Some(v.into_boxed_slice());
    }
    let slot = &mut table.as_mut().expect("just installed")[n as usize];
    slot.buf[..cut].copy_from_slice(&s.as_bytes()[..cut]);
    slot.len = cut;
}

/// Read slot `n`. `n >= SLOTS`, and any slot before the first write,
/// reads as empty — so a pattern asking for `textSlot(99)` draws nothing
/// rather than erroring.
pub fn with_slot<R>(n: u8, f: impl FnOnce(&str) -> R) -> R {
    // SAFETY: single-writer rule.
    let table = unsafe { &*TABLE.0.get() };
    match table {
        Some(t) if (n as usize) < SLOTS => {
            let slot = &t[n as usize];
            // SAFETY: `set_slot` only ever stores whole characters.
            f(unsafe { core::str::from_utf8_unchecked(&slot.buf[..slot.len]) })
        }
        _ => f(""),
    }
}

/// Clear every slot — the mirror's per-session reset and the tests'.
pub fn clear_slots() {
    for n in 0..SLOTS as u8 {
        set_slot(n, "");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::vec;

    /// Render `text` into a `w×h` row-major grid as ASCII art, `#` = ink.
    fn art(text: &str, font: Font, w: u16, h: u16, x: i32, y: i32) -> String {
        let grid = GridMap {
            w,
            h,
            serpentine: false,
        };
        let mut px = vec![[0u8; 3]; w as usize * h as usize];
        draw(&mut px, &grid, x, y, text, font, [255, 255, 255]);
        let mut out = String::new();
        for r in 0..h as usize {
            for c in 0..w as usize {
                out.push(if px[r * w as usize + c] == [0, 0, 0] {
                    '.'
                } else {
                    '#'
                });
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn fonts_are_well_formed_psf2() {
        for f in FONTS {
            let blob = f.blob();
            assert_eq!(blob[..4], PSF2_MAGIC, "{} magic", f.as_str());
            let p = Psf2::parse(blob);
            assert_eq!(p.length, LAST_CP - FIRST_CP + 1, "{} glyph count", f.as_str());
            assert_eq!(
                p.charsize,
                p.width.div_ceil(8) * p.height,
                "{} charsize",
                f.as_str()
            );
            assert_eq!(
                p.glyphs.len(),
                (p.length * p.charsize) as usize,
                "{} body length",
                f.as_str()
            );
            // a blank sheet would pass every check above
            assert!(p.glyphs.iter().any(|&b| b != 0), "{} is blank", f.as_str());
        }
    }

    #[test]
    fn total_font_bytes() {
        let n: usize = FONTS.iter().map(|f| f.blob().len()).sum();
        assert_eq!(n, 2091, "font blob size (report it when it moves)");
    }

    #[test]
    fn metrics_are_the_documented_ones() {
        assert_eq!(
            (
                Font::Tiny.cell_width(),
                Font::Tiny.height(),
                Font::Tiny.advance()
            ),
            (3, 6, 4)
        );
        assert_eq!(
            (
                Font::Regular.cell_width(),
                Font::Regular.height(),
                Font::Regular.advance()
            ),
            (5, 7, 6)
        );
        assert_eq!(
            (
                Font::Large.cell_width(),
                Font::Large.height(),
                Font::Large.advance()
            ),
            (5, 8, 6)
        );
        for f in FONTS {
            assert_eq!(Font::from_wire(f.as_str()), Some(f));
        }
        assert_eq!(Font::from_wire("comic sans"), None);
    }

    #[test]
    fn tiny_glyphs_match_the_bitmap() {
        // Tom Thumb "A" then "1", 3-wide cells with a 1 px gap
        assert_eq!(
            art("A1", Font::Tiny, 8, 6, 0, 0),
            "\
.#...#..
#.#.##..
###..#..
#.#..#..
#.#..#..
........
"
        );
    }

    #[test]
    fn regular_glyphs_match_the_bitmap() {
        // X11 misc-fixed 5x7 "Hi" — the `i` keeps its serif base
        assert_eq!(
            art("Hi", Font::Regular, 12, 7, 0, 0),
            "\
#..#....#...
#..#........
####...##...
#..#....#...
#..#....#...
#..#...###..
............
"
        );
    }

    #[test]
    fn large_glyphs_match_the_bitmap() {
        // Spleen 5x8 zero — slashed, which is how you tell it from `O`
        assert_eq!(
            art("0", Font::Large, 6, 8, 0, 0),
            "\
......
.##...
#..#..
#.##..
##.#..
#..#..
.##...
......
"
        );
    }

    #[test]
    fn unknown_code_points_draw_a_question_mark() {
        assert_eq!(
            art("\u{2603}", Font::Tiny, 4, 6, 0, 0),
            art("?", Font::Tiny, 4, 6, 0, 0)
        );
    }

    #[test]
    fn width_is_chars_times_advance() {
        assert_eq!(width("", Font::Regular), 0);
        assert_eq!(width("HI", Font::Tiny), 8);
        assert_eq!(width("HI", Font::Regular), 12);
        assert_eq!(width("HI", Font::Large), 12);
        // a non-ASCII char still occupies one cell (it draws `?`)
        assert_eq!(width("\u{2603}", Font::Large), 6);
    }

    #[test]
    fn clips_at_all_four_edges() {
        let ink = |s: &str, x: i32, y: i32| {
            art(s, Font::Regular, 8, 8, x, y)
                .chars()
                .filter(|&c| c == '#')
                .count()
        };
        let whole = ink("A", 0, 0);
        assert!(whole > 0);
        // fully off each edge
        assert_eq!(ink("A", -8, 0), 0, "left");
        assert_eq!(ink("A", 8, 0), 0, "right");
        assert_eq!(ink("A", 0, -8), 0, "top");
        assert_eq!(ink("A", 0, 8), 0, "bottom");
        // partly off each edge: some ink, less than the whole glyph
        for (x, y, edge) in [(-2, 0, "left"), (6, 0, "right"), (0, -2, "top"), (0, 6, "bottom")] {
            let n = ink("A", x, y);
            assert!(n > 0 && n < whole, "{edge}: {n} of {whole}");
        }
    }

    #[test]
    fn a_serpentine_grid_draws_the_same_picture() {
        let straight = GridMap {
            w: 8,
            h: 8,
            serpentine: false,
        };
        let snake = GridMap {
            w: 8,
            h: 8,
            serpentine: true,
        };
        let mut a = vec![[0u8; 3]; 64];
        let mut b = vec![[0u8; 3]; 64];
        draw(&mut a, &straight, 1, 1, "A", Font::Tiny, [9, 9, 9]);
        draw(&mut b, &snake, 1, 1, "A", Font::Tiny, [9, 9, 9]);
        // same cells lit, addressed through each grid's own index order
        for r in 0..8 {
            for c in 0..8 {
                assert_eq!(
                    a[straight.index(r, c)],
                    b[snake.index(r, c)],
                    "cell ({r},{c})"
                );
            }
        }
    }

    #[test]
    fn alignment_anchors_the_run() {
        let w = width("AB", Font::Tiny) as i32; // 8
        let grid = GridMap {
            w: 32,
            h: 6,
            serpentine: false,
        };
        let first_lit_col = |align: Align, x: i32| {
            let mut px = vec![[0u8; 3]; 32 * 6];
            draw_aligned(&mut px, &grid, x, 0, "AB", Font::Tiny, [1, 1, 1], align);
            (0..32).find(|&c| (0..6).any(|r| px[r * 32 + c] != [0; 3]))
        };
        assert_eq!(first_lit_col(Align::Left, 10), Some(10));
        assert_eq!(first_lit_col(Align::Center, 10), Some(10 - w as usize / 2));
        assert_eq!(first_lit_col(Align::Right, 20), Some(20 - w as usize));
    }

    #[test]
    fn number_formatting_table() {
        let mut b = TextString::new();
        let f = |v: f64, digits: u8, decimals: u8| {
            let mut b = TextString::new();
            format_number(Fx::from_f64(v), digits, decimals, &mut b);
            String::from(b.as_str())
        };
        assert_eq!(f(0.0, 1, 0), "0");
        assert_eq!(f(7.0, 1, 0), "7");
        assert_eq!(f(7.0, 3, 0), "007");
        assert_eq!(f(-7.0, 2, 0), "-07");
        assert_eq!(f(12.0, 1, 0), "12");
        assert_eq!(f(0.5, 1, 2), "0.50");
        assert_eq!(f(-0.5, 1, 1), "-0.5");
        assert_eq!(f(3.14159, 1, 2), "3.14");
        assert_eq!(f(3.14159, 1, 3), "3.142");
        assert_eq!(f(1.0 / 3.0, 1, 4), "0.3333");
        // rounding carries into the integer part
        assert_eq!(f(0.999, 1, 2), "1.00");
        assert_eq!(f(9.999, 2, 2), "10.00");
        // -0.4 at zero decimals is 0, not "-0"
        assert_eq!(f(-0.4, 1, 0), "0");
        // caps
        format_number(Fx::from_f64(1.5), 99, 99, &mut b);
        assert_eq!(b.as_str(), "0000000001.5000");
    }

    #[test]
    fn clock_formats() {
        let c = |fmt: ClockFmt| String::from(format_clock(fmt, 9, 5, 3, 2026, 9, 24).as_str());
        assert_eq!(c(ClockFmt::Hm24), "09:05");
        assert_eq!(c(ClockFmt::Hms24), "09:05:03");
        assert_eq!(c(ClockFmt::Hm12), "9:05");
        assert_eq!(c(ClockFmt::Hms12), "9:05:03");
        assert_eq!(c(ClockFmt::MonthDay), "09-24");
        assert_eq!(c(ClockFmt::Date), "2026-09-24");
        // 12-hour wrap and midnight
        assert_eq!(format_clock(ClockFmt::Hm12, 0, 0, 0, 0, 1, 1).as_str(), "12:00");
        assert_eq!(format_clock(ClockFmt::Hm12, 13, 7, 0, 0, 1, 1).as_str(), "1:07");
        assert_eq!(format_clock(ClockFmt::Hm24, 23, 59, 0, 0, 1, 1).as_str(), "23:59");
        // no time sync: out-of-range fields clamp
        assert_eq!(format_clock(ClockFmt::Hms24, 99, 99, 99, 0, 0, 0).as_str(), "23:59:59");
        for f in CLOCK_FMTS {
            assert_eq!(ClockFmt::from_wire(f.as_str()), Some(f));
        }
        assert_eq!(ClockFmt::from_wire("swatch beats"), None);
    }

    #[test]
    fn slots_round_trip_and_truncate_on_a_char_boundary() {
        clear_slots();
        with_slot(0, |s| assert_eq!(s, ""));
        set_slot(0, "hello");
        with_slot(0, |s| assert_eq!(s, "hello"));
        // other slots are untouched
        with_slot(1, |s| assert_eq!(s, ""));
        // an empty write clears
        set_slot(0, "");
        with_slot(0, |s| assert_eq!(s, ""));
        // out of range: ignored on write, empty on read
        set_slot(SLOTS as u8, "nope");
        with_slot(SLOTS as u8, |s| assert_eq!(s, ""));
        // 64 B of ASCII fits exactly
        let long: String = core::iter::repeat_n('x', 70).collect();
        set_slot(7, &long);
        with_slot(7, |s| assert_eq!(s.len(), SLOT_MAX));
        // a 3-byte character straddling the limit is dropped whole
        let mut s = String::new();
        for _ in 0..21 {
            s.push('\u{2603}'); // 3 bytes each -> 63
        }
        s.push('\u{2603}'); // would end at 66
        set_slot(2, &s);
        with_slot(2, |v| {
            assert_eq!(v.len(), 63);
            assert_eq!(v.chars().count(), 21);
        });
        clear_slots();
    }
}
