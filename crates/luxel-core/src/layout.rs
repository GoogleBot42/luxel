//! The Layout — the ONE geometry object a host configures (Gitea #465,
//! design proposal §2 / §5.3 / §5.3b / §5.4d).
//!
//! Before this module the same concept was spread over four endpoints —
//! `/api/config` (pixel count), `/api/map grid` (the grid), `/api/datapin`
//! and `/api/protocol` (the wiring) — with nothing naming the shape itself.
//! A Layout is:
//!
//! ```text
//! kind    strip | matrix | map          ← "custom map" is a coordinate SOURCE
//! matrix  pw ph cols rows start dir snake rot180 [scan]
//! out     n pin proto order count [rev] ← wiring segments of ONE pixel space
//! proj    proj1d proj2d proj3d          ← §5.4d defaults (crate::projection)
//! ```
//!
//! The parser, the validator, the JSON writer and the flash record all live
//! here so the firmware and the `luxel serve` mirror can only differ in the
//! FACTS they feed in ([`Limits`], [`View`]) — the same arrangement
//! [`crate::caps`] and [`crate::projection`] use.
//!
//! **What this module does NOT own.** The pixel count and the pixel map are
//! already persisted by every host (an nvs record and the map blob), and a
//! Layout must not become a second copy of them: [`parse`] returns the
//! *edits* to make ([`Edit::pixels`], [`Edit::map`]) and the host applies
//! them through the paths it already has. That is what keeps `/api/config`
//! and `/api/map` honest as aliases rather than drifting shadows.

use alloc::string::String;
use alloc::vec::Vec;

use crate::jsonview::{push_piece, push_u32};
use crate::outpipe::{ColorOrder, GridMap};
use crate::projection::{Projection, ProjectionMode};

/// What shape the installation IS. `Map` covers both a coordinate cloud and
/// a procedural grid installed as a map — the distinction the UI needs there
/// is `regular`, not a second kind.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayoutKind {
    Strip,
    Matrix,
    Map,
}

impl LayoutKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            LayoutKind::Strip => "strip",
            LayoutKind::Matrix => "matrix",
            LayoutKind::Map => "map",
        }
    }
    pub fn from_str(s: &str) -> Option<LayoutKind> {
        match s {
            "strip" => Some(LayoutKind::Strip),
            "matrix" => Some(LayoutKind::Matrix),
            "map" => Some(LayoutKind::Map),
            _ => None,
        }
    }
}

/// Which corner the chain (or the pixel run) starts from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Corner {
    Tl,
    Tr,
    Bl,
    Br,
}

impl Corner {
    pub const fn as_str(self) -> &'static str {
        match self {
            Corner::Tl => "tl",
            Corner::Tr => "tr",
            Corner::Bl => "bl",
            Corner::Br => "br",
        }
    }
    pub fn from_str(s: &str) -> Option<Corner> {
        match s {
            "tl" => Some(Corner::Tl),
            "tr" => Some(Corner::Tr),
            "bl" => Some(Corner::Bl),
            "br" => Some(Corner::Br),
            _ => None,
        }
    }
}

/// Whether the run advances along a row (x first) or a column (y first).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RunDir {
    Row,
    Col,
}

impl RunDir {
    pub const fn as_str(self) -> &'static str {
        match self {
            RunDir::Row => "row",
            RunDir::Col => "col",
        }
    }
    pub fn from_str(s: &str) -> Option<RunDir> {
        match s {
            "row" => Some(RunDir::Row),
            "col" => Some(RunDir::Col),
            _ => None,
        }
    }
}

/// The matrix arrangement (proposal §5.3, S3c). `pw`×`ph` is ONE panel (or,
/// on a strip-built matrix with `cols`=`rows`=1, the whole grid); `cols`×
/// `rows` tile them; `start`/`dir`/`snake`/`rot180` describe how the chain
/// threads the tiles — and, in the one-tile case, how the pixel run threads
/// the grid, which is the same widget one level down.
///
/// Stored and reported by #465; the boot-time panel→pixel remap that makes a
/// snaked multi-panel chain real is #475.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Matrix {
    pub pw: u16,
    pub ph: u16,
    pub cols: u8,
    pub rows: u8,
    pub start: Corner,
    pub dir: RunDir,
    pub snake: bool,
    pub rot180: bool,
    /// HUB75 scan divisor (16 = 1/16 scan); 0 = the board's own.
    pub scan: u8,
}

impl Matrix {
    /// A single `w`×`h` tile, wired top-left, row-major, no snake — the
    /// arrangement every existing device is running today.
    pub const fn single(w: u16, h: u16) -> Matrix {
        Matrix {
            pw: w,
            ph: h,
            cols: 1,
            rows: 1,
            start: Corner::Tl,
            dir: RunDir::Row,
            snake: false,
            rot180: false,
            scan: 0,
        }
    }

    /// Total grid width in pixels.
    pub const fn width(&self) -> u32 {
        self.pw as u32 * self.cols as u32
    }
    /// Total grid height in pixels.
    pub const fn height(&self) -> u32 {
        self.ph as u32 * self.rows as u32
    }
    /// Pixels the arrangement covers.
    pub const fn pixels(&self) -> u32 {
        self.width() * self.height()
    }
    /// Tiles in the chain.
    pub const fn panels(&self) -> u32 {
        self.cols as u32 * self.rows as u32
    }

    /// The part #475 consumes: everything about how the chain threads the
    /// tiles. Two arrangements that differ only in `pw`/`ph` resize the grid
    /// (live), they do not rewire it (reboot).
    const fn wiring(&self) -> (u8, u8, u8, u8, bool, bool, u8) {
        (
            self.cols,
            self.rows,
            self.start as u8,
            self.dir as u8,
            self.snake,
            self.rot180,
            self.scan,
        )
    }
}

/// One physical LED output — a consecutive run of the ONE pixel space
/// (proposal §5.3b / D11). `count` is pixels on a strip Layout and tiles on
/// a matrix Layout; `rev` marks a run wired backwards.
///
/// Stored, validated and reported by #465; driven — one driver instance per
/// output, the frame split by run — by #474.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Output {
    /// Output index, 0-based, `< caps.outputs`.
    pub n: u8,
    pub pin: u8,
    /// Host protocol code (`leds::Protocol::as_u8` on the firmware).
    pub proto: u8,
    /// [`ColorOrder`] code.
    pub order: u8,
    pub count: u32,
    pub rev: bool,
}

/// The whole configured Layout, minus the pixel count and the map payload —
/// see the module docs for why those stay where they already live.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Layout {
    pub kind: LayoutKind,
    /// Meaningful when `kind` is [`LayoutKind::Matrix`]; carried across a
    /// kind change so switching to Strip and back does not lose the wiring.
    pub matrix: Matrix,
    /// Empty = "one output, the host's defaults" (see [`View`]).
    pub outputs: Vec<Output>,
    pub proj: Projection,
}

impl Layout {
    /// The Layout a host with nothing stored comes up in: `kind` from the
    /// board, one implicit output, projection defaults (all no-ops).
    pub fn board_default(kind: LayoutKind, matrix: Matrix) -> Layout {
        Layout { kind, matrix, outputs: Vec::new(), proj: Projection::DEFAULT }
    }

    /// Whether moving from `self` to `next` needs a reboot to take effect:
    /// the chain wiring (#475) and the output DRIVER INSTANCES (#474) are
    /// built once at boot. Everything else applies on the next frame — the
    /// pixel count, the grid, the map, the projection defaults, the run each
    /// output drives (`count`, `rev`, re-read from the Layout by
    /// `write_frame` every frame) and output 0's protocol and colour order,
    /// because output 0 IS the strip the aliases describe: the render task
    /// reconfigures its SPI and the output chain permutes the frame into its
    /// order (Gitea #550).
    ///
    /// So an `out` line is compared on the three fields a BOOT reads: the
    /// output exists, its pad, and — on a FURTHER output only — the wire
    /// format its peripheral was configured for (`firmware/src/output.rs`,
    /// `Chan`, which copies both when it is built).
    ///
    /// An EMPTY table is the one implicit output on `default_pin`
    /// ([`Limits::default_pin`]), so a body that merely writes that same
    /// output down explicitly rebuilds nothing — which is what lets a
    /// Settings page POST the whole table on every edit.
    pub fn reboot_required(&self, next: &Layout, default_pin: u8) -> bool {
        if next.kind == LayoutKind::Matrix
            && (self.kind != LayoutKind::Matrix
                || self.matrix.wiring() != next.matrix.wiring())
        {
            return true;
        }
        let implicit = [Output { n: 0, pin: default_pin, proto: 0, order: 0, count: 0, rev: false }];
        let a = if self.outputs.is_empty() { &implicit[..] } else { &self.outputs[..] };
        let b = if next.outputs.is_empty() { &implicit[..] } else { &next.outputs[..] };
        if a.len() != b.len() {
            return true;
        }
        for (x, y) in a.iter().zip(b) {
            if x.n != y.n || x.pin != y.pin {
                return true;
            }
            if x.n != 0 && (x.proto != y.proto || x.order != y.order) {
                return true;
            }
        }
        false
    }

    /// The Layout as its own `POST` wire — what a host persists, the way the
    /// playlist persists its body verbatim. [`parse`] reads it back, so there
    /// is no second codec to keep in step and no version to migrate: a stored
    /// body that this build's grammar rejects simply leaves the host on its
    /// board default (the store's no-migration rule).
    ///
    /// `pixels` is the count to write into the `strip` line. A reader ignores
    /// it — the pixel count lives in the host's own record — but writing it
    /// keeps the text a legal body someone can POST back.
    pub fn to_wire(&self, pixels: u32, proto_name: &dyn Fn(u8) -> &'static str) -> String {
        let mut out = String::new();
        match self.kind {
            LayoutKind::Strip => {
                push_piece(&mut out, "strip ");
                push_u32(&mut out, pixels);
            }
            // `map` alone would CLEAR the map if this were POSTed; a reader
            // of the stored form drops `Edit::map`, which is what makes that
            // safe (see the `Edit` docs).
            LayoutKind::Map => push_piece(&mut out, "map"),
            LayoutKind::Matrix => {
                let m = &self.matrix;
                push_piece(&mut out, "matrix ");
                for v in [m.pw as u32, m.ph as u32, m.cols as u32, m.rows as u32] {
                    push_u32(&mut out, v);
                    out.push(' ');
                }
                push_piece(&mut out, m.start.as_str());
                out.push(' ');
                push_piece(&mut out, m.dir.as_str());
                out.push(' ');
                for v in [u32::from(m.snake), u32::from(m.rot180), m.scan as u32] {
                    push_u32(&mut out, v);
                    out.push(' ');
                }
            }
        }
        for o in &self.outputs {
            push_piece(&mut out, "\nout ");
            for v in [o.n as u32, o.pin as u32] {
                push_u32(&mut out, v);
                out.push(' ');
            }
            push_piece(&mut out, proto_name(o.proto));
            out.push(' ');
            push_piece(&mut out, ColorOrder(o.order).name());
            out.push(' ');
            push_u32(&mut out, o.count);
            if o.rev {
                push_piece(&mut out, " rev");
            }
        }
        for (field, mode) in [
            ("\nproj1d ", self.proj.proj1d),
            ("\nproj2d ", self.proj.proj2d),
            ("\nproj3d ", self.proj.proj3d),
        ] {
            push_piece(&mut out, field);
            push_piece(&mut out, mode.as_str());
        }
        out
    }
}

/// The consecutive slice of the ONE pixel space that one output puts on its
/// wire (proposal §5.3b / D11, Gitea #474). `rev` means the run is wired
/// backwards: its first physical LED is the run's LAST pixel.
///
/// The arithmetic lives here rather than in the firmware so it is testable
/// on the host and so the mirror describes the same split the device drives.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Run {
    pub start: u32,
    pub len: u32,
    pub rev: bool,
}

impl Run {
    /// Drives nothing — an output past the end of a stale table.
    pub const NONE: Run = Run { start: 0, len: 0, rev: false };

    /// Trimmed to what a `pixels`-long frame actually holds. A stored table
    /// can outlive the pixel count that made it add up (an alias moved the
    /// count — see [`Limits::strict`]), and a driver must clamp rather than
    /// index past the frame.
    pub fn clamped(self, pixels: u32) -> Run {
        let start = self.start.min(pixels);
        Run { start, len: self.len.min(pixels - start), rev: self.rev }
    }

    /// Frame index of this run's `i`-th wire pixel. `rev` walks it
    /// backwards; `i` must be `< len`.
    pub fn index(&self, i: u32) -> u32 {
        self.start + if self.rev { self.len - 1 - i } else { i }
    }
}

impl Layout {
    /// The run output `n` drives: the configured outputs laid end to end in
    /// `n` order, `count` units each, clamped to the live pixel count.
    ///
    /// `None` = this output drives nothing (no such entry). An EMPTY table
    /// is the one implicit output covering the whole space — the state a
    /// host with nothing stored is in, which is why output 0 always gets a
    /// run there and every other output gets `None`.
    ///
    /// On a matrix Layout a `count` is TILES, so one unit is one panel's
    /// worth of pixels; on a strip or map Layout a unit is one pixel.
    pub fn run_of(&self, n: u8, pixels: u32) -> Option<Run> {
        if self.outputs.is_empty() {
            return (n == 0).then_some(Run { start: 0, len: pixels, rev: false });
        }
        let unit = match self.kind {
            LayoutKind::Matrix => (self.matrix.pw as u32).saturating_mul(self.matrix.ph as u32),
            _ => 1,
        };
        let mut start: u32 = 0;
        for o in &self.outputs {
            let len = o.count.saturating_mul(unit);
            if o.n == n {
                return Some(Run { start, len, rev: o.rev }.clamped(pixels));
            }
            start = start.saturating_add(len).min(pixels);
        }
        None
    }
}

// --- POST /api/layout -------------------------------------------------------

/// A rejected body: which line (1-based, 0 = the body as a whole) and why.
/// The message is `&'static str` so building one allocates nothing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LayoutError {
    pub line: u32,
    pub msg: &'static str,
}

/// The host facts [`parse`] validates against. Pins and protocol names are
/// closures because they are genuinely per-host (the board's reserved set,
/// the driver's alias table) — everything else is a number.
pub struct Limits<'a> {
    /// Board pixel ceiling (`/api/status`'s `max_pixels`).
    pub max_pixels: u32,
    /// Physical outputs the board has (`caps.outputs`).
    pub outputs: u8,
    /// A HUB75 board: it IS a matrix, so `strip` is refused and there is no
    /// configurable strip output to describe with `out`.
    pub panel: bool,
    /// May this GPIO carry a strip data line (`board::data_pin_ok`)?
    pub pin_ok: &'a dyn Fn(u8) -> bool,
    /// The pad the host's ONE implicit output is on — [`View::default_pin`],
    /// i.e. what an EMPTY output table means here. It is what lets
    /// [`Layout::reboot_required`] tell a body that only writes the implicit
    /// output down explicitly from one that moves the pad. 0 on a host with
    /// no data pin of its own (the mirror).
    pub default_pin: u8,
    /// Protocol name → host code, aliases included.
    pub proto_code: &'a dyn Fn(&str) -> Option<u8>,
    /// Enforce the cross-line invariant that the outputs partition the pixel
    /// space exactly. True for a `POST`; **false when a host re-reads its own
    /// persisted Layout**, where the partition can have gone stale behind an
    /// alias that moved the pixel count — losing the whole stored Layout over
    /// that would be worse than reporting a table that does not add up, which
    /// the next POST has to restate anyway.
    pub strict: bool,
}

/// What a POST asks the host to do. The Layout itself plus the two edits
/// that belong to state this module does not own.
#[derive(Debug)]
pub struct Edit {
    pub layout: Layout,
    /// New pixel count to apply + persist (the `/api/config` path).
    pub pixels: Option<u32>,
    /// New `POST /api/map` body to apply + persist (the `/api/map` path):
    /// `""` clears, `grid W H` installs a procedural grid, anything else is
    /// the coordinate form.
    ///
    /// A host reading its own PERSISTED Layout ([`Layout::to_wire`]) drops
    /// this and `pixels`: the map and the pixel count have their own storage
    /// and are already loaded by then. That is what lets the stored form
    /// abbreviate a map Layout to the bare word `map`.
    pub map: Option<String>,
    /// True when a stored field only a reboot applies changed — the chain
    /// wiring (#475) or an output's DRIVER INSTANCE (#474): which outputs
    /// exist, the pad each is bound to, and the wire format a further
    /// output's peripheral was built for. See [`Layout::reboot_required`].
    pub reboot_required: bool,
}

/// Parse a `POST /api/layout` body against the current Layout.
///
/// Lines (blank lines and `#` comments ignored, order free, at most one kind
/// line):
///
/// ```text
/// strip <pixels>
/// matrix <pw> <ph> <cols> <rows> <start> <dir> <snake> <rot180> [<scan>]
/// map [grid <w> <h> | <dims> <raw16.16…>]
/// out <n> <pin> <proto> <order> <count> [rev]
/// proj1d|proj2d|proj3d <index|x|y|z|xy|xz|yz>
/// ```
///
/// `out` lines are all-or-nothing: one of them replaces the whole table, so
/// a body with none keeps the outputs untouched.
pub fn parse(
    body: &str,
    cur: &Layout,
    cur_pixels: u32,
    lim: &Limits,
) -> Result<Edit, LayoutError> {
    let mut next = cur.clone();
    let mut pixels = None;
    let mut map = None;
    let mut outs: Option<Vec<Output>> = None;
    let mut kind_seen = false;

    for (i, raw) in body.lines().enumerate() {
        let line = i as u32 + 1;
        let text = raw.trim();
        if text.is_empty() || text.starts_with('#') {
            continue;
        }
        let err = |msg| LayoutError { line, msg };
        let mut it = text.split_whitespace();
        let verb = it.next().unwrap_or("");
        match verb {
            "strip" | "matrix" | "map" if kind_seen => {
                return Err(err("only one strip/matrix/map line per body"))
            }
            "strip" => {
                kind_seen = true;
                if lim.panel {
                    return Err(err("this board is a matrix: strip is not a Layout it can take"));
                }
                // the count is optional: `strip` alone means "become a strip,
                // keep the pixels" — which is also how the persisted form
                // stays a legal body
                if let Some(tok) = it.next() {
                    let n = num(tok).ok_or(err("expected: strip [pixels]"))?;
                    if n < 1 || n > lim.max_pixels {
                        return Err(err("pixels out of range for this board"));
                    }
                    pixels = Some(n);
                }
                next.kind = LayoutKind::Strip;
                map = Some(String::new()); // a strip Layout has no map
            }
            "matrix" => {
                kind_seen = true;
                let m = parse_matrix(&mut it).ok_or(err(
                    "expected: matrix <pw> <ph> <cols> <rows> <tl|tr|bl|br> <row|col> <0|1> <0|1> [scan]",
                ))?;
                if m.pixels() < 1 || m.pixels() > lim.max_pixels {
                    return Err(err("pw*ph*cols*rows out of range for this board"));
                }
                if m.width() > u16::MAX as u32 || m.height() > u16::MAX as u32 {
                    return Err(err("grid is wider or taller than 65535"));
                }
                next.kind = LayoutKind::Matrix;
                next.matrix = m;
                pixels = Some(m.pixels());
                let mut wire = String::from("grid ");
                push_u32(&mut wire, m.width());
                wire.push(' ');
                push_u32(&mut wire, m.height());
                map = Some(wire);
            }
            "map" => {
                kind_seen = true;
                next.kind = LayoutKind::Map;
                // everything after `map` is exactly a POST /api/map body
                let rest = text["map".len()..].trim();
                map = Some(String::from(rest));
                if let Some(n) = grid_pixels(rest) {
                    if n > lim.max_pixels {
                        return Err(err("grid is larger than this board's pixel ceiling"));
                    }
                    pixels = Some(n);
                }
            }
            "out" => {
                if lim.panel {
                    return Err(err("this board has no configurable strip output"));
                }
                // `out none` empties the table, back to the ONE implicit
                // output built from the host's live strip settings — the
                // state a board with nothing stored is in. Without it there
                // is no way back once a table exists, which a Settings page
                // (and a device restored to found state) needs.
                if text == "out none" {
                    outs = Some(Vec::new());
                    continue;
                }
                let o = parse_out(&mut it, lim).ok_or(err(
                    "expected: out <n> <pin> <sk9822|ws2812> <rgb|rbg|grb|gbr|brg|bgr> <count> [rev]",
                ))?;
                if o.n >= lim.outputs {
                    return Err(err("output index is past this board's output count"));
                }
                if !(lim.pin_ok)(o.pin) {
                    return Err(err("pin is reserved or not an output on this board"));
                }
                if o.count < 1 {
                    return Err(err("output count must be at least 1"));
                }
                let list = outs.get_or_insert_with(Vec::new);
                if list.iter().any(|e: &Output| e.n == o.n) {
                    return Err(err("duplicate output index"));
                }
                // Two outputs on one pad would be two drivers fighting over
                // it — the firmware binds a peripheral per output at boot
                // (#474), so this has to be caught here, not there.
                if list.iter().any(|e: &Output| e.pin == o.pin) {
                    return Err(err("two outputs cannot share a data pin"));
                }
                // Insert in index order rather than sorting afterwards: the
                // list is at most `caps.outputs` long, and `sort_by_key`
                // instantiates driftsort, which is a 4 KiB stack frame in
                // the firmware's web task (tools/stack-check.sh).
                let at = list.iter().position(|e: &Output| e.n > o.n).unwrap_or(list.len());
                list.insert(at, o);
            }
            // All three are accepted whatever the kind, because every
            // PERSISTED Layout carries all three (`to_wire`) and a body this
            // grammar rejects would drop the host back to its board default.
            // Which of them is in FORCE is the projection table's business,
            // not the parser's: since #538 `proj3d` never is, and `proj2d`
            // only on a 3D Layout (`projection_options`).
            "proj1d" | "proj2d" | "proj3d" => {
                let mode: ProjectionMode = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or(err("expected one of index|x|y|z|xy|xz|yz"))?;
                let d = match verb {
                    "proj3d" => 3,
                    "proj2d" => 2,
                    _ => 1,
                };
                next.proj.set(d, mode);
            }
            _ => return Err(err("unknown line (want strip|matrix|map|out|proj1d|proj2d|proj3d)")),
        }
    }

    if let Some(list) = outs {
        // Outputs partition ONE pixel space (D11), so their counts must add
        // up to it exactly — pixels on a strip, tiles on a matrix.
        let want = match next.kind {
            LayoutKind::Matrix => next.matrix.panels(),
            _ => pixels.unwrap_or(cur_pixels),
        };
        let sum: u32 = list.iter().map(|o| o.count).sum();
        if lim.strict && !list.is_empty() && sum != want {
            return Err(LayoutError {
                line: 0,
                msg: "output counts must add up to the Layout's pixels (strip) or panels (matrix)",
            });
        }
        next.outputs = list;
    }

    let reboot_required = cur.reboot_required(&next, lim.default_pin);
    Ok(Edit { layout: next, pixels, map, reboot_required })

}
fn parse_matrix<'a>(it: &mut impl Iterator<Item = &'a str>) -> Option<Matrix> {
    let pw = u16::try_from(num(it.next()?)?).ok()?;
    let ph = u16::try_from(num(it.next()?)?).ok()?;
    let cols = u8::try_from(num(it.next()?)?).ok()?;
    let rows = u8::try_from(num(it.next()?)?).ok()?;
    let start = Corner::from_str(it.next()?)?;
    let dir = RunDir::from_str(it.next()?)?;
    let snake = flag(it.next()?)?;
    let rot180 = flag(it.next()?)?;
    let scan = match it.next() {
        None => 0,
        Some(v) => u8::try_from(num(v)?).ok()?,
    };
    if pw == 0 || ph == 0 || cols == 0 || rows == 0 {
        return None;
    }
    Some(Matrix { pw, ph, cols, rows, start, dir, snake, rot180, scan })
}

fn parse_out<'a>(it: &mut impl Iterator<Item = &'a str>, lim: &Limits) -> Option<Output> {
    let n = u8::try_from(num(it.next()?)?).ok()?;
    let pin = u8::try_from(num(it.next()?)?).ok()?;
    let proto = (lim.proto_code)(it.next()?)?;
    let order = ColorOrder::from_name(it.next()?)?.0;
    let count = num(it.next()?)?;
    let rev = match it.next() {
        None => false,
        Some("rev") => true,
        Some(_) => return None,
    };
    Some(Output { n, pin, proto, order, count, rev })
}

/// A decimal `u32` — no sign, no radix, no `ParseIntError`.
///
/// `str::parse` instantiates `from_str_radix` once per integer WIDTH, and
/// this grammar wants three of them; on a board with 3 % of its OTA slot
/// left that is real money (docs/boards.md). Every number here is a small
/// unsigned count, so the callers narrow with `try_from`.
fn num(s: &str) -> Option<u32> {
    let b = s.as_bytes();
    if b.is_empty() || b.len() > 10 {
        return None;
    }
    let mut v: u32 = 0;
    for &c in b {
        let d = c.wrapping_sub(b'0');
        if d > 9 {
            return None;
        }
        v = v.checked_mul(10)?.checked_add(d as u32)?;
    }
    Some(v)
}

fn flag(s: &str) -> Option<bool> {
    match s {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
    }
}

/// Pixels a `grid W H` map body covers, `None` for any other body.
fn grid_pixels(body: &str) -> Option<u32> {
    let mut it = body.split_whitespace();
    if it.next()? != "grid" {
        return None;
    }
    let w = num(it.next()?)?;
    let h = num(it.next()?)?;
    w.checked_mul(h).filter(|n| *n > 0)
}

// --- GET /api/layout --------------------------------------------------------

/// The host-side facts the JSON needs that a [`Layout`] does not carry: the
/// live pixel count, the installed map, and what one implicit output means
/// on this host.
pub struct View<'a> {
    /// The pixel count to report. `GET` passes the APPLIED count (what
    /// `/api/config` reports); a `POST` answer passes the REQUESTED one,
    /// because the render task has not drained the resize yet and a client
    /// must not be told its POST did nothing.
    pub pixels: u32,
    pub max_pixels: u32,
    /// Dimensionality of the installed map (0 = none).
    pub map_dims: u8,
    /// The installed map's grid, when it is one.
    pub map_grid: Option<GridMap>,
    /// The `GET /api/map` body, embedded verbatim so a client needs one
    /// fetch rather than two.
    pub map_json: &'a str,
    pub proto_name: &'a dyn Fn(u8) -> &'static str,
    /// The single output a host with no stored table has: its active data
    /// pin, protocol and colour order (`/api/config` + `/api/output`).
    pub default_pin: u8,
    pub default_proto: u8,
    pub default_order: u8,
    /// What this host's panel driver makes of a matrix Layout (#475).
    /// `None` on a host with no panel — a strip mirror, and every strip
    /// board. Absent entirely without the `panel` feature.
    #[cfg(feature = "panel")]
    pub panel: Option<PanelView>,
}

/// The panel driver's own reading of the configured arrangement (#475): the
/// firmware reports it so a UI computing the same estimate in the browser
/// has something to check itself against, and so it can say when a board is
/// driving fewer tiles than the arrangement describes.
#[cfg(feature = "panel")]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PanelView {
    /// Estimated rescan rate for the whole configured chain, Hz. The formula
    /// is in docs/api.md; below ~100 Hz the panel visibly flickers.
    pub est_hz: u32,
    /// Leading tiles of the chain this board's framebuffer actually drives.
    /// Less than `panels` means the rest of the arrangement is dark (#401).
    pub drive: u32,
}

impl Layout {
    /// The Layout's OWN dimensionality, width, height and regularity — what
    /// the installation IS, not what the running pattern is being rendered
    /// as. `/api/status`'s `geom` is the latter and they differ whenever a
    /// projection or the engine's fabricated √n grid is in play.
    pub fn shape(&self, v: &View) -> (u8, u32, u32, bool) {
        match self.kind {
            LayoutKind::Strip => (1, v.pixels, 1, true),
            LayoutKind::Matrix => (2, self.matrix.width(), self.matrix.height(), true),
            LayoutKind::Map => match v.map_grid {
                Some(g) => (v.map_dims.max(2), g.w as u32, g.h as u32, true),
                None => (v.map_dims.max(1), 0, 0, false),
            },
        }
    }

    /// `GET /api/layout` (and the body a `POST` answers with). Allocation is
    /// the caller's `String`; every field is pushed, never formatted.
    pub fn push_json(&self, out: &mut String, v: &View) {
        let (dims, w, h, regular) = self.shape(v);
        push_piece(out, "{\"kind\":\"");
        push_piece(out, self.kind.as_str());
        push_piece(out, "\",\"source\":\"");
        push_piece(out, if self.kind == LayoutKind::Map { "map" } else { "regular" });
        push_piece(out, "\",\"dims\":");
        push_u32(out, dims as u32);
        push_piece(out, ",\"regular\":");
        push_piece(out, if regular { "true" } else { "false" });
        push_piece(out, ",\"pixels\":");
        push_u32(out, v.pixels);
        push_piece(out, ",\"max\":");
        push_u32(out, v.max_pixels);
        push_piece(out, ",\"w\":");
        push_u32(out, w);
        push_piece(out, ",\"h\":");
        push_u32(out, h);
        if self.kind == LayoutKind::Matrix {
            push_piece(out, ",\"matrix\":{\"pw\":");
            push_u32(out, self.matrix.pw as u32);
            push_piece(out, ",\"ph\":");
            push_u32(out, self.matrix.ph as u32);
            push_piece(out, ",\"cols\":");
            push_u32(out, self.matrix.cols as u32);
            push_piece(out, ",\"rows\":");
            push_u32(out, self.matrix.rows as u32);
            push_piece(out, ",\"start\":\"");
            push_piece(out, self.matrix.start.as_str());
            push_piece(out, "\",\"dir\":\"");
            push_piece(out, self.matrix.dir.as_str());
            push_piece(out, "\",\"snake\":");
            push_u32(out, u32::from(self.matrix.snake));
            push_piece(out, ",\"rot180\":");
            push_u32(out, u32::from(self.matrix.rot180));
            push_piece(out, ",\"scan\":");
            push_u32(out, self.matrix.scan as u32);
            #[cfg(feature = "panel")]
            if let Some(p) = v.panel {
                push_piece(out, ",\"est_hz\":");
                push_u32(out, p.est_hz);
                push_piece(out, ",\"drive\":");
                push_u32(out, p.drive);
            }
            push_piece(out, "}");
        }
        push_piece(out, ",\"outputs\":[");
        if self.outputs.is_empty() {
            let count = match self.kind {
                LayoutKind::Matrix => self.matrix.panels(),
                _ => v.pixels,
            };
            push_output(
                out,
                &Output {
                    n: 0,
                    pin: v.default_pin,
                    proto: v.default_proto,
                    order: v.default_order,
                    count,
                    rev: false,
                },
                v,
            );
        } else {
            for (i, o) in self.outputs.iter().enumerate() {
                if i > 0 {
                    push_piece(out, ",");
                }
                push_output(out, o, v);
            }
        }
        push_piece(out, "],\"proj\":{\"proj1d\":\"");
        push_piece(out, self.proj.proj1d.as_str());
        push_piece(out, "\",\"proj2d\":\"");
        push_piece(out, self.proj.proj2d.as_str());
        push_piece(out, "\",\"proj3d\":\"");
        push_piece(out, self.proj.proj3d.as_str());
        push_piece(out, "\"},\"map\":");
        push_piece(out, v.map_json);
        push_piece(out, "}");
    }
}

fn push_output(out: &mut String, o: &Output, v: &View) {
    push_piece(out, "{\"n\":");
    push_u32(out, o.n as u32);
    push_piece(out, ",\"pin\":");
    push_u32(out, o.pin as u32);
    push_piece(out, ",\"proto\":\"");
    push_piece(out, (v.proto_name)(o.proto));
    push_piece(out, "\",\"order\":\"");
    push_piece(out, ColorOrder(o.order).name());
    push_piece(out, "\",\"count\":");
    push_u32(out, o.count);
    push_piece(out, ",\"rev\":");
    push_piece(out, if o.rev { "true" } else { "false" });
    push_piece(out, "}");
}

/// `{"ok":false,"error":"…","line":N}` — the one rejection shape, so both
/// hosts (and the e2e harness) can key off `line`.
pub fn push_error_json(out: &mut String, e: &LayoutError) {
    push_piece(out, "{\"ok\":false,\"error\":\"");
    push_piece(out, e.msg);
    push_piece(out, "\",\"line\":");
    push_u32(out, e.line);
    push_piece(out, "}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proto_code(s: &str) -> Option<u8> {
        match s {
            "sk9822" | "apa102" => Some(0),
            "ws2812" | "ws2811" | "ws2815" | "ws281x" => Some(1),
            _ => None,
        }
    }
    fn proto_name(c: u8) -> &'static str {
        if c == 1 {
            "ws2812"
        } else {
            "sk9822"
        }
    }
    fn pin_ok(p: u8) -> bool {
        p != 5 && p < 40
    }

    fn strip_limits() -> Limits<'static> {
        Limits {
            max_pixels: 2048,
            outputs: 2,
            panel: false,
            pin_ok: &pin_ok,
            default_pin: 18,
            proto_code: &proto_code,
            strict: true,
        }
    }
    /// A ceiling big enough for the multi-panel arrangements below.
    fn big_limits() -> Limits<'static> {
        Limits { max_pixels: 1 << 16, ..strip_limits() }
    }
    fn panel_limits() -> Limits<'static> {
        Limits {
            max_pixels: 4096,
            outputs: 1,
            panel: true,
            pin_ok: &pin_ok,
            default_pin: 0,
            proto_code: &proto_code,
            strict: true,
        }
    }

    fn strip_layout() -> Layout {
        Layout::board_default(LayoutKind::Strip, Matrix::single(1, 1))
    }

    fn view<'a>(l: &Layout, pixels: u32, map_json: &'a str) -> View<'a> {
        let _ = l;
        View {
            pixels,
            max_pixels: 2048,
            map_dims: 0,
            map_grid: None,
            map_json,
            proto_name: &proto_name,
            default_pin: 18,
            default_proto: 1,
            default_order: 2,
            #[cfg(feature = "panel")]
            panel: None,
        }
    }

    #[test]
    fn strip_line_sets_pixels_and_clears_the_map() {
        let cur = strip_layout();
        let e = parse("strip 120", &cur, 60, &strip_limits()).unwrap();
        assert_eq!(e.layout.kind, LayoutKind::Strip);
        assert_eq!(e.pixels, Some(120));
        assert_eq!(e.map.as_deref(), Some(""));
        assert!(!e.reboot_required, "pixel count is live");
    }

    #[test]
    fn matrix_line_derives_pixels_and_a_grid_map() {
        let cur = strip_layout();
        let e = parse("matrix 32 16 2 1 tl row 1 0", &cur, 60, &strip_limits()).unwrap();
        assert_eq!(e.layout.kind, LayoutKind::Matrix);
        assert_eq!((e.layout.matrix.width(), e.layout.matrix.height()), (64, 16));
        assert_eq!(e.pixels, Some(1024));
        assert_eq!(e.map.as_deref(), Some("grid 64 16"));
        assert!(e.reboot_required, "the chain wiring is built at boot (#475)");
    }

    #[test]
    fn resizing_a_panel_is_live_but_rewiring_is_not() {
        let mut cur = strip_layout();
        cur.kind = LayoutKind::Matrix;
        cur.matrix = Matrix::single(32, 32);
        // same wiring, bigger tile → live
        let e = parse("matrix 64 64 1 1 tl row 0 0", &cur, 1024, &panel_limits()).unwrap();
        assert!(!e.reboot_required);
        // same tile, snaked → reboot
        let e = parse("matrix 32 32 1 1 tl row 1 0", &cur, 1024, &panel_limits()).unwrap();
        assert!(e.reboot_required);
    }

    #[test]
    fn map_line_passes_its_body_through() {
        let cur = strip_layout();
        let e = parse("map grid 8 8", &cur, 60, &strip_limits()).unwrap();
        assert_eq!(e.layout.kind, LayoutKind::Map);
        assert_eq!(e.map.as_deref(), Some("grid 8 8"));
        assert_eq!(e.pixels, Some(64));
        let e = parse("map 2 0 0 65536 0", &cur, 60, &strip_limits()).unwrap();
        assert_eq!(e.map.as_deref(), Some("2 0 0 65536 0"));
        assert_eq!(e.pixels, None, "a coordinate map does not resize the strip");
        let e = parse("map", &cur, 60, &strip_limits()).unwrap();
        assert_eq!(e.map.as_deref(), Some(""), "bare `map` clears");
    }

    #[test]
    fn outputs_must_partition_the_pixel_space() {
        let cur = strip_layout();
        let body = "strip 120\nout 0 18 ws2812 grb 60\nout 1 19 ws2812 grb 60 rev";
        let e = parse(body, &cur, 60, &strip_limits()).unwrap();
        assert_eq!(e.layout.outputs.len(), 2);
        assert!(e.layout.outputs[1].rev);
        assert!(e.reboot_required, "the output table is built at boot (#474)");

        let bad = "strip 120\nout 0 18 ws2812 grb 60";
        assert_eq!(parse(bad, &cur, 60, &strip_limits()).unwrap_err().line, 0);
    }

    #[test]
    fn two_outputs_cannot_share_a_pad() {
        let cur = strip_layout();
        let body = "strip 120\nout 0 18 ws2812 grb 60\nout 1 18 ws2812 grb 60";
        let e = parse(body, &cur, 60, &strip_limits()).unwrap_err();
        assert_eq!(e.line, 3);
        assert!(e.msg.contains("share a data pin"), "{}", e.msg);
    }

    // --- the split each output drives (#474) --------------------------------

    fn two_outputs(a: u32, b: u32, rev: bool) -> Layout {
        let mut l = strip_layout();
        l.outputs.push(Output { n: 0, pin: 18, proto: 1, order: 2, count: a, rev: false });
        l.outputs.push(Output { n: 1, pin: 17, proto: 1, order: 2, count: b, rev });
        l
    }

    #[test]
    fn an_empty_table_is_one_output_over_the_whole_space() {
        let l = strip_layout();
        assert_eq!(l.run_of(0, 60), Some(Run { start: 0, len: 60, rev: false }));
        assert_eq!(l.run_of(1, 60), None, "a board with nothing stored drives one");
    }

    #[test]
    fn outputs_take_consecutive_runs_in_index_order() {
        let l = two_outputs(30, 30, false);
        assert_eq!(l.run_of(0, 60), Some(Run { start: 0, len: 30, rev: false }));
        assert_eq!(l.run_of(1, 60), Some(Run { start: 30, len: 30, rev: false }));
        assert_eq!(l.run_of(2, 60), None);
        // uneven runs are fine — the counts, not the halves, decide
        let l = two_outputs(50, 10, false);
        assert_eq!(l.run_of(1, 60), Some(Run { start: 50, len: 10, rev: false }));
    }

    #[test]
    fn a_reversed_run_walks_its_slice_backwards() {
        let l = two_outputs(30, 30, true);
        let r = l.run_of(1, 60).unwrap();
        assert!(r.rev);
        // wire pixel 0 is the LAST pixel of the run, wire pixel 29 the first
        assert_eq!(r.index(0), 59);
        assert_eq!(r.index(29), 30);
        // and a forward run is the identity over its slice
        let f = l.run_of(0, 60).unwrap();
        assert_eq!((f.index(0), f.index(29)), (0, 29));
    }

    #[test]
    fn a_stale_table_is_clamped_never_indexed_past_the_frame() {
        // an alias moved the pixel count under a table that added up to 60
        let l = two_outputs(30, 30, false);
        assert_eq!(l.run_of(0, 40), Some(Run { start: 0, len: 30, rev: false }));
        assert_eq!(l.run_of(1, 40), Some(Run { start: 30, len: 10, rev: false }));
        // …and past the first run entirely: a zero-length run, not a panic
        assert_eq!(l.run_of(1, 20), Some(Run { start: 20, len: 0, rev: false }));
        assert_eq!(l.run_of(0, 20), Some(Run { start: 0, len: 20, rev: false }));
        assert_eq!(Run::NONE.clamped(0), Run::NONE);
    }

    #[test]
    fn a_matrix_outputs_count_is_tiles_so_a_run_is_panels_of_pixels() {
        let mut l = Layout::board_default(LayoutKind::Matrix, Matrix::single(32, 16));
        l.matrix.cols = 3;
        l.outputs.push(Output { n: 0, pin: 18, proto: 1, order: 2, count: 2, rev: false });
        l.outputs.push(Output { n: 1, pin: 17, proto: 1, order: 2, count: 1, rev: false });
        assert_eq!(l.run_of(0, 1536), Some(Run { start: 0, len: 1024, rev: false }));
        assert_eq!(l.run_of(1, 1536), Some(Run { start: 1024, len: 512, rev: false }));
    }

    #[test]
    fn out_none_empties_the_table() {
        let mut cur = strip_layout();
        cur.outputs.push(Output { n: 0, pin: 18, proto: 1, order: 2, count: 60, rev: false });
        cur.outputs.push(Output { n: 1, pin: 19, proto: 1, order: 2, count: 60, rev: true });
        let e = parse("out none", &cur, 120, &strip_limits()).unwrap();
        assert!(e.layout.outputs.is_empty(), "back to the one implicit output");
        assert!(e.reboot_required, "the table is built at boot (#474)");
        // and the emptied table does not have to add up to anything
        assert_eq!(e.pixels, None);
    }

    // --- what a reboot actually builds (#550) -------------------------------

    /// A two-output table to POST variations of; 120 px, both on ws2812/grb.
    const TWO: &str = "out 0 18 ws2812 grb 60\nout 1 19 ws2812 grb 60";

    fn two_output_cur() -> Layout {
        parse(TWO, &strip_layout(), 120, &strip_limits()).unwrap().layout
    }

    /// `reboot_required` for `body` applied to a 120 px two-output device.
    fn reboot(body: &str) -> bool {
        parse(body, &two_output_cur(), 120, &strip_limits()).unwrap().reboot_required
    }

    #[test]
    fn writing_the_implicit_output_down_explicitly_rebuilds_nothing() {
        // the one implicit output IS `out 0 <default_pin> … <pixels>`, so a
        // Settings page that always POSTs the whole table must not be told
        // to reboot for it
        let cur = strip_layout();
        let e = parse("out 0 18 ws2812 grb 60", &cur, 60, &strip_limits()).unwrap();
        assert!(!e.reboot_required, "same output, same pad");
        // …and back again
        let e = parse("out none", &e.layout, 60, &strip_limits()).unwrap();
        assert!(!e.reboot_required);
        // a DIFFERENT pad is a rebind, which only a boot does
        let e = parse("out 0 19 ws2812 grb 60", &cur, 60, &strip_limits()).unwrap();
        assert!(e.reboot_required, "the SPI driver binds MOSI once, at boot");
    }

    #[test]
    fn repartitioning_the_pixel_space_is_live() {
        // the run boundaries are re-read from the Layout every frame
        assert!(!reboot("out 0 18 ws2812 grb 90\nout 1 19 ws2812 grb 30"));
        assert!(!reboot("out 0 18 ws2812 grb 30\nout 1 19 ws2812 grb 90"));
    }

    #[test]
    fn reversing_a_run_is_live() {
        // `rev` rides in the same `Run` the frame loop re-reads
        assert!(!reboot("out 0 18 ws2812 grb 60 rev\nout 1 19 ws2812 grb 60"));
        assert!(!reboot("out 0 18 ws2812 grb 60\nout 1 19 ws2812 grb 60 rev"));
    }

    #[test]
    fn output_0s_protocol_and_colour_order_are_live() {
        // output 0 IS the strip the aliases drive: the render task
        // reconfigures its SPI and the output chain permutes into its order
        assert!(!reboot("out 0 18 sk9822 grb 60\nout 1 19 ws2812 grb 60"));
        assert!(!reboot("out 0 18 ws2812 bgr 60\nout 1 19 ws2812 grb 60"));
    }

    #[test]
    fn a_further_outputs_wire_format_waits_for_its_driver() {
        // output 1's SPI clock and colour order are captured when its
        // peripheral is built (firmware/src/output.rs `Chan`)
        assert!(reboot("out 0 18 ws2812 grb 60\nout 1 19 sk9822 grb 60"));
        assert!(reboot("out 0 18 ws2812 grb 60\nout 1 19 ws2812 bgr 60"));
    }

    #[test]
    fn moving_a_data_pad_needs_a_reboot() {
        assert!(reboot("out 0 17 ws2812 grb 60\nout 1 19 ws2812 grb 60"));
        assert!(reboot("out 0 18 ws2812 grb 60\nout 1 17 ws2812 grb 60"));
    }

    #[test]
    fn gaining_or_losing_a_driver_instance_needs_a_reboot() {
        // one output's driver is never built…
        assert!(reboot("out 0 18 ws2812 grb 120"));
        // …and `out none` gives the same answer for the same reason
        assert!(reboot("out none"));
        // the reverse direction too
        let one = parse("out 0 18 ws2812 grb 120", &strip_layout(), 120, &strip_limits())
            .unwrap()
            .layout;
        assert!(parse(TWO, &one, 120, &strip_limits()).unwrap().reboot_required);
    }

    #[test]
    fn a_body_that_touches_no_output_leaves_the_answer_to_the_rest() {
        // `out` lines are all-or-nothing: a proj-only body keeps the table
        assert!(!reboot("proj1d y"));
        // and the chain wiring still rebuilds on its own
        assert!(parse("matrix 8 8 2 1 tl row 1 0", &strip_layout(), 64, &strip_limits())
            .unwrap()
            .reboot_required);
    }

    #[test]
    fn matrix_outputs_count_panels_not_pixels() {
        let cur = strip_layout();
        let body = "matrix 32 32 2 1 tl row 0 0\nout 0 18 ws2812 grb 1\nout 1 19 ws2812 grb 1";
        assert!(parse(body, &cur, 60, &strip_limits()).is_ok());
    }

    #[test]
    fn bad_lines_name_their_line_number() {
        let cur = strip_layout();
        let cases: [(&str, u32); 6] = [
            ("strip 0", 1),
            ("strip 9999", 1),
            ("strip 60\nout 0 5 ws2812 grb 60", 2), // pin 5 reserved
            ("strip 60\nout 9 18 ws2812 grb 60", 2), // output index
            ("strip 60\nout 0 18 apa106 grb 60", 2), // unknown protocol
            ("\n\nwibble", 3),
        ];
        for (body, line) in cases {
            let e = parse(body, &cur, 60, &strip_limits()).unwrap_err();
            assert_eq!(e.line, line, "body {body:?}");
        }
    }

    #[test]
    fn one_kind_line_per_body() {
        let cur = strip_layout();
        let e = parse("strip 60\nmatrix 8 8 1 1 tl row 0 0", &cur, 60, &strip_limits())
            .unwrap_err();
        assert_eq!(e.line, 2);
    }

    #[test]
    fn a_panel_board_refuses_strip_and_out() {
        let cur = Layout::board_default(LayoutKind::Matrix, Matrix::single(64, 64));
        assert_eq!(parse("strip 60", &cur, 4096, &panel_limits()).unwrap_err().line, 1);
        assert_eq!(
            parse("out 0 18 ws2812 grb 4096", &cur, 4096, &panel_limits()).unwrap_err().line,
            1
        );
    }

    #[test]
    fn projection_lines_round_trip() {
        let cur = strip_layout();
        let e = parse("proj1d x\nproj2d y\nproj3d yz", &cur, 60, &strip_limits()).unwrap();
        assert_eq!(e.layout.proj.proj1d, ProjectionMode::X);
        assert_eq!(e.layout.proj.proj2d, ProjectionMode::Y);
        assert_eq!(e.layout.proj.proj3d, ProjectionMode::Yz);
        assert!(!e.reboot_required, "projection defaults are live");
        assert_eq!(e.pixels, None);
        assert_eq!(e.map, None, "a proj-only body leaves the map alone");
    }

    #[test]
    fn the_persisted_wire_round_trips() {
        let mut l = Layout::board_default(LayoutKind::Matrix, Matrix::single(64, 32));
        l.matrix.cols = 3;
        l.matrix.snake = true;
        l.matrix.start = Corner::Br;
        l.matrix.dir = RunDir::Col;
        l.matrix.scan = 16;
        l.proj = Projection::new(ProjectionMode::X, ProjectionMode::Y, ProjectionMode::Xz);
        l.outputs.push(Output { n: 0, pin: 18, proto: 1, order: 2, count: 3, rev: false });
        l.outputs.push(Output { n: 1, pin: 19, proto: 0, order: 5, count: 3, rev: true });
        let wire = l.to_wire(6144, &proto_name);
        assert_eq!(
            wire,
            "matrix 64 32 3 1 br col 1 0 16 \n\
             out 0 18 ws2812 grb 3\nout 1 19 sk9822 bgr 3 rev\n\
             proj1d x\nproj2d y\nproj3d xz"
        );
        // the host reloads with its OWN board default + limits, exactly as
        // `layout::init()` does
        let fresh = Layout::board_default(LayoutKind::Strip, Matrix::single(1, 1));
        let load = Limits { strict: false, ..big_limits() };
        let back = parse(&wire, &fresh, 6144, &load).unwrap();
        assert_eq!(back.layout, l);

        // a strip Layout's stored form keeps its pixels readable but the
        // reader takes the count from its own record
        let s = Layout::board_default(LayoutKind::Strip, Matrix::single(1, 1));
        assert_eq!(s.to_wire(120, &proto_name), "strip 120\nproj1d index\nproj2d z\nproj3d xy");
        // …and `strip` with no count is legal, which is what makes a map
        // Layout's bare `map` line safe to abbreviate
        assert_eq!(parse("strip", &s, 120, &strip_limits()).unwrap().pixels, None);

        let m = Layout::board_default(LayoutKind::Map, Matrix::single(1, 1));
        assert!(m.to_wire(64, &proto_name).starts_with("map\n"));
        let back = parse(&m.to_wire(64, &proto_name), &fresh, 64, &strip_limits()).unwrap();
        assert_eq!(back.layout.kind, LayoutKind::Map);
        assert_eq!(back.map.as_deref(), Some(""), "the reader drops this");
    }

    #[test]
    fn an_unreadable_stored_wire_leaves_the_board_default() {
        let fresh = Layout::board_default(LayoutKind::Strip, Matrix::single(1, 1));
        // a body this build's grammar rejects (a future kind, say)
        assert!(parse("lattice 4 4 4", &fresh, 60, &strip_limits()).is_err());
    }

    #[test]
    fn strip_json_shape() {
        let l = strip_layout();
        let mut s = String::new();
        l.push_json(&mut s, &view(&l, 60, "{\"installed\":false,\"dims\":0,\"count\":0}"));
        assert_eq!(
            s,
            "{\"kind\":\"strip\",\"source\":\"regular\",\"dims\":1,\"regular\":true,\
             \"pixels\":60,\"max\":2048,\"w\":60,\"h\":1,\
             \"outputs\":[{\"n\":0,\"pin\":18,\"proto\":\"ws2812\",\"order\":\"grb\",\
             \"count\":60,\"rev\":false}],\
             \"proj\":{\"proj1d\":\"index\",\"proj2d\":\"z\",\"proj3d\":\"xy\"},\
             \"map\":{\"installed\":false,\"dims\":0,\"count\":0}}"
        );
    }

    #[test]
    fn matrix_json_carries_the_arrangement() {
        let l = Layout::board_default(LayoutKind::Matrix, Matrix::single(64, 64));
        let mut s = String::new();
        l.push_json(&mut s, &view(&l, 4096, "{}"));
        assert!(s.contains("\"kind\":\"matrix\""));
        assert!(s.contains("\"matrix\":{\"pw\":64,\"ph\":64,\"cols\":1,\"rows\":1,\"start\":\"tl\",\"dir\":\"row\",\"snake\":0,\"rot180\":0,\"scan\":0}"));
        assert!(s.contains("\"w\":64,\"h\":64"));
        assert!(s.contains("\"count\":1"), "one implicit output = one panel");
        assert!(!s.contains("est_hz"), "no panel driver, no estimate");
    }

    /// A host with a panel driver adds its reading of the arrangement (#475).
    #[cfg(feature = "panel")]
    #[test]
    fn a_panel_host_adds_the_refresh_estimate_and_the_driven_count() {
        let mut l = Layout::board_default(LayoutKind::Matrix, Matrix::single(64, 64));
        l.matrix.cols = 2;
        let mut v = view(&l, 8192, "null");
        v.panel = Some(PanelView { est_hz: 57, drive: 1 });
        let mut s = String::new();
        l.push_json(&mut s, &v);
        assert!(s.contains("\"rot180\":0,\"scan\":0,\"est_hz\":57,\"drive\":1}"), "{s}");
        assert!(s.contains("\"w\":128,\"h\":64"));
        assert!(s.contains("\"count\":2"), "one implicit output covers both panels");
    }

    #[test]
    fn map_json_reports_the_detected_grid() {
        let l = Layout::board_default(LayoutKind::Map, Matrix::single(1, 1));
        let mut s = String::new();
        let mut v = view(&l, 128, "{}");
        v.map_dims = 2;
        v.map_grid = Some(GridMap { w: 16, h: 8, serpentine: false });
        l.push_json(&mut s, &v);
        assert!(s.contains("\"source\":\"map\""));
        assert!(s.contains("\"dims\":2,\"regular\":true"));
        assert!(s.contains("\"w\":16,\"h\":8"));
        assert!(!s.contains("\"matrix\":"), "no arrangement off a matrix Layout");
    }

    #[test]
    fn irregular_map_has_no_shape() {
        let l = Layout::board_default(LayoutKind::Map, Matrix::single(1, 1));
        let mut s = String::new();
        let mut v = view(&l, 300, "{}");
        v.map_dims = 3;
        l.push_json(&mut s, &v);
        assert!(s.contains("\"dims\":3,\"regular\":false"));
        assert!(s.contains("\"w\":0,\"h\":0"));
    }

    #[test]
    fn error_json_shape() {
        let mut s = String::new();
        push_error_json(&mut s, &LayoutError { line: 2, msg: "nope" });
        assert_eq!(s, "{\"ok\":false,\"error\":\"nope\",\"line\":2}");
    }
}
