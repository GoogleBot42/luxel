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

/// The driver chip a HUB75 panel's shift registers actually are (Gitea
/// #525). Most panels are a plain shift register and need no init at all;
/// the rest want a register write clocked in before the first frame, which
/// is why this is a stored SETTING and not a board constant — one firmware
/// image drives all of them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Chip {
    /// No init sequence: FM6124, SM16208, ICN2037 and every other plain
    /// shift-register driver.
    ShiftReg,
    Fm6126a,
    /// Same two-register init as [`Chip::Fm6126a`].
    Icn2038s,
    /// Its own init, and the latch is held for the last 3 clocks of every
    /// row rather than 1 — see [`PanelDriver::latch_clocks`].
    Dp3246,
}

impl Chip {
    /// Every chip, in wire order — what `GET /api/layout` offers as
    /// `driver.chips` so a UI never hard-codes the list.
    pub const ALL: [Chip; 4] = [Chip::ShiftReg, Chip::Fm6126a, Chip::Icn2038s, Chip::Dp3246];

    pub const fn as_str(self) -> &'static str {
        match self {
            Chip::ShiftReg => "shiftreg",
            Chip::Fm6126a => "fm6126a",
            Chip::Icn2038s => "icn2038s",
            Chip::Dp3246 => "dp3246",
        }
    }
    pub fn parse(s: &str) -> Option<Chip> {
        match s {
            "shiftreg" => Some(Chip::ShiftReg),
            "fm6126a" => Some(Chip::Fm6126a),
            "icn2038s" => Some(Chip::Icn2038s),
            "dp3246" => Some(Chip::Dp3246),
            _ => None,
        }
    }
}

/// How a HUB75 panel is DRIVEN, as opposed to how it is arranged: the BCM
/// bit depth, the LCD_CAM pixel clock, the driver chip's init and how long
/// OE is held off around the latch (Gitea #401 + #525, the `panel` wire
/// line). Every field was a compile-time constant before this; all four are
/// read at boot, so changing any of them is `reboot_required`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PanelDriver {
    /// BCM bitplanes, 4..=8. Fewer is a faster rescan and a coarser ramp:
    /// one rescan shifts the whole chain `2^planes - 1` times.
    pub planes: u8,
    /// LCD_CAM pixel clock in MHz, 2..=40. 40 does not survive an FM6124
    /// panel (the bench table in `firmware/src/hub75.rs`); the firmware
    /// takes the number anyway and lets the UI warn.
    pub clock_mhz: u8,
    pub chip: Chip,
    /// Clocks at the START of every row block, and again just before the
    /// latch word, where OE is OFF — 0..=8. 1 is the stock template.
    pub blank: u8,
}

impl Default for PanelDriver {
    /// The board default every HUB75 image shipped before the `panel` line
    /// existed: 7 planes, 30 MHz, no chip init, one blanking clock.
    fn default() -> PanelDriver {
        PanelDriver { planes: 7, clock_mhz: 30, chip: Chip::ShiftReg, blank: 1 }
    }
}

impl PanelDriver {
    pub const fn clock_hz(&self) -> u32 {
        self.clock_mhz as u32 * 1_000_000
    }

    /// Clocks the latch is held HIGH at the end of a row block — 1 for a
    /// shift register, 3 for a DP3246.
    pub const fn latch_clocks(&self) -> u8 {
        match self.chip {
            Chip::Dp3246 => 3,
            _ => 1,
        }
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
    /// Meaningful when the host has a HUB75 panel; carried like `matrix`,
    /// defaulted when the wire says nothing (the `panel` line is optional,
    /// so a body without one KEEPS the stored driver — [`Edit::driver_set`]).
    pub driver: PanelDriver,
}

impl Layout {
    /// The Layout a host with nothing stored comes up in: `kind` from the
    /// board, one implicit output, projection defaults (all no-ops).
    pub fn board_default(kind: LayoutKind, matrix: Matrix) -> Layout {
        Layout {
            kind,
            matrix,
            outputs: Vec::new(),
            proj: Projection::DEFAULT,
            driver: PanelDriver::default(),
        }
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
    ///
    /// `panel_board` is the host saying "my framebuffer IS this Layout"
    /// ([`Limits::panel`]): on a HUB75 board the DMA framebuffer is
    /// allocated from the arrangement at boot, so `pw`/`ph` are
    /// reboot-required there as well ([`Layout::fb_geometry_changed`]).
    /// False — every strip host — gives byte-identical answers to the
    /// pre-#525 two-argument form, `pw`/`ph` included: on a strip-built
    /// matrix they only resize the grid, which is live.
    pub fn reboot_required(&self, next: &Layout, default_pin: u8, panel_board: bool) -> bool {
        if next.kind == LayoutKind::Matrix
            && (self.kind != LayoutKind::Matrix
                || self.matrix.wiring() != next.matrix.wiring()
                // the `panel` line: all four fields are read once, at boot
                || self.driver != next.driver)
        {
            return true;
        }
        if panel_board && self.fb_geometry_changed(next) {
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

    /// Whether the DMA framebuffer a HUB75 boot allocates would differ:
    /// the panel size, the chain and the scan depth (`fb_cols` × `fb_rows`
    /// in the driver, docs/api.md). [`Layout::reboot_required`] already ORs
    /// this in when a host tells it `panel_board`; it is public so a firmware
    /// can ask the same question on its own, outside a POST.
    pub fn fb_geometry_changed(&self, next: &Layout) -> bool {
        if next.kind != LayoutKind::Matrix {
            return false;
        }
        if self.kind != LayoutKind::Matrix {
            return true;
        }
        let g = |m: &Matrix| (m.pw, m.ph, m.cols, m.rows, m.scan);
        g(&self.matrix) != g(&next.matrix)
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
                // The driver is only meaningful for a matrix, and the stored
                // record IS the wire, so it goes out whenever the kind does
                // — a reader without a `panel` line keeps its own default.
                let d = &self.driver;
                push_piece(&mut out, "\npanel ");
                for v in [d.planes as u32, d.clock_mhz as u32] {
                    push_u32(&mut out, v);
                    out.push(' ');
                }
                push_piece(&mut out, d.chip.as_str());
                out.push(' ');
                push_u32(&mut out, d.blank as u32);
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
    /// Whether the body carried a `panel` line. The MERGE rule needs no work
    /// from a host — [`parse`] starts from the Layout it was given, so a body
    /// without one leaves `layout.driver` exactly as it was stored rather
    /// than resetting it to the board default — and this is the flag for a
    /// host that wants to know anyway: to log the change, or to skip
    /// re-persisting a driver nobody touched.
    pub driver_set: bool,
    /// A `proj` line: the projection the RUNNING pattern is to be shown
    /// under, right now (Gitea #598). `Some(Some(mode))` installs an
    /// override in the slot for the running pattern's OWN dimensionality,
    /// `Some(None)` drops back to the `proj1d/2d/3d` defaults, `None` is
    /// "the body said nothing about it".
    ///
    /// Deliberately NOT part of [`Layout`]: it is a property of what is
    /// running, not of the rig, so it is never persisted, never in
    /// [`Layout::to_wire`] and never `reboot_required`. It rides on this
    /// endpoint because the host that owns the Layout is the host that owns
    /// the engine — and because a separate route costs the tightest board in
    /// the fleet more OTA slot than it has (docs/boards.md, Gitea #543).
    pub proj_now: Option<Option<ProjectionMode>>,
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
/// panel <planes> <clock_mhz> <chip> <blank>
/// out <n> <pin> <proto> <order> <count> [rev]
/// proj1d|proj2d|proj3d <index|x|y|z|xy|xz|yz>
/// ```
///
/// `out` lines are all-or-nothing: one of them replaces the whole table, so
/// a body with none keeps the outputs untouched. `panel` is a merge for the
/// same reason in reverse: a body without one keeps the stored driver
/// ([`Edit::driver_set`]).
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
    let mut driver_set = false;
    let mut proj_now = None;

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
                // A HUB75 panel shifts two half-height rows at once through
                // R1G1B1/R2G2B2, so its address depth is `ph / 2` — a panel
                // with an odd `ph` is not a shape the driver can express.
                if lim.panel && m.ph % 2 != 0 {
                    return Err(err("a panel's ph must be even"));
                }
                // …and a stated `scan` shallower than that stripes the
                // framebuffer, `(ph / 2) / scan` stripes of it, which only
                // works if it tiles exactly (docs/api.md, Gitea #401).
                if m.scan != 0 && (m.ph % 2 != 0 || (m.ph / 2) % m.scan as u16 != 0) {
                    return Err(err("scan must divide ph/2"));
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
            // How the panel is DRIVEN (#525). Accepted whatever the kind,
            // like the `proj*` lines and for the same reason: every
            // persisted matrix Layout carries it (`to_wire`) and a body this
            // grammar rejects would drop the host to its board default.
            "panel" => {
                if driver_set {
                    return Err(err("only one panel line per body"));
                }
                driver_set = true;
                next.driver = parse_driver(&mut it).map_err(err)?;
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
            // The RUNNING pattern's override (#598): ephemeral, so it goes
            // to the engine and nowhere near the stored Layout. `default`
            // (or a bare `proj`) clears it back to the defaults above.
            "proj" => {
                proj_now = Some(match it.next().unwrap_or("default") {
                    "default" => None,
                    // the same literal the proj1d/2d/3d arm uses, so the
                    // linker merges it rather than adding a second copy
                    t => Some(
                        t.parse()
                            .ok()
                            .ok_or(err("expected one of index|x|y|z|xy|xz|yz"))?,
                    ),
                });
            }
            _ => {
                return Err(err(
                    "unknown line (want strip|matrix|map|panel|out|proj|proj1d|proj2d|proj3d)",
                ))
            }
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

    let reboot_required = cur.reboot_required(&next, lim.default_pin, lim.panel);
    Ok(Edit { layout: next, pixels, map, reboot_required, driver_set, proj_now })
}

/// The `panel` line's four fields. Each range gets its own message, because
/// "expected: panel …" tells a UI nothing about which number it got wrong.
fn parse_driver<'a>(it: &mut impl Iterator<Item = &'a str>) -> Result<PanelDriver, &'static str> {
    const USAGE: &str =
        "expected: panel <planes> <clock_mhz> <shiftreg|fm6126a|icn2038s|dp3246> <blank>";
    let planes = it.next().and_then(num).ok_or(USAGE)?;
    if !(4..=8).contains(&planes) {
        return Err("panel: planes must be 4..8");
    }
    let clock_mhz = it.next().and_then(num).ok_or(USAGE)?;
    if !(2..=40).contains(&clock_mhz) {
        return Err("panel: clock_mhz must be 2..40");
    }
    let chip = Chip::parse(it.next().ok_or(USAGE)?)
        .ok_or("panel: chip must be shiftreg|fm6126a|icn2038s|dp3246")?;
    let blank = it.next().and_then(num).ok_or(USAGE)?;
    if blank > 8 {
        return Err("panel: blank must be 0..8");
    }
    Ok(PanelDriver { planes: planes as u8, clock_mhz: clock_mhz as u8, chip, blank: blank as u8 })
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
    /// What the RUNNING driver was actually built with (#525), against which
    /// a UI compares the configured [`Layout::driver`] to know a reboot is
    /// pending. `None` = there is no panel output at all: the framebuffer
    /// allocation or the LCD_CAM init failed even at the board default.
    pub driver_live: Option<LiveDriver>,
}

/// The driver the firmware actually booted — the live half of the `driver`
/// block (#525). Every field is read back from the running DMA setup, never
/// from the stored Layout, which is the whole point: `configured != live`
/// is how a UI knows a reboot is pending, and `fallback` is how it knows the
/// configured shape did not fit.
#[cfg(feature = "panel")]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LiveDriver {
    pub planes: u8,
    pub clock_mhz: u8,
    pub chip: Chip,
    pub blank: u8,
    /// The framebuffer's chain extent in pixels: `w` = `pw` × chain length,
    /// `h` = `ph`.
    pub w: u16,
    pub h: u16,
    /// Address rows the driver scans.
    pub scan: u16,
    /// Bytes of ONE framebuffer (there are two, double-buffered).
    pub fb_bytes: u32,
    /// The configured geometry/driver did not fit in internal RAM and the
    /// firmware booted the board default instead.
    pub fallback: bool,
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
        // The driver belongs to the BOARD, not to the kind: a HUB75 board
        // with a coordinate map installed is still driving a panel.
        #[cfg(feature = "panel")]
        if let Some(p) = v.panel {
            push_driver_json(out, &self.driver, &p);
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

/// The `driver` block (#525): the CONFIGURED driver, the chip list so a UI
/// never hard-codes it, and what the firmware actually booted — `null` there
/// when the panel output is off entirely. Only a host with a panel driver
/// emits it, so a strip board's body is byte-identical to one built before
/// this existed.
#[cfg(feature = "panel")]
fn push_driver_json(out: &mut String, d: &PanelDriver, p: &PanelView) {
    push_piece(out, ",\"driver\":{");
    push_driver_fields(out, d.planes, d.clock_mhz, d.chip, d.blank);
    push_piece(out, ",\"chips\":[");
    for (i, c) in Chip::ALL.iter().enumerate() {
        push_piece(out, if i > 0 { ",\"" } else { "\"" });
        push_piece(out, c.as_str());
        push_piece(out, "\"");
    }
    push_piece(out, "],\"live\":");
    match &p.driver_live {
        None => push_piece(out, "null"),
        Some(l) => {
            push_piece(out, "{");
            push_driver_fields(out, l.planes, l.clock_mhz, l.chip, l.blank);
            for (field, v) in [
                (",\"w\":", l.w as u32),
                (",\"h\":", l.h as u32),
                (",\"scan\":", l.scan as u32),
                (",\"fb_bytes\":", l.fb_bytes),
            ] {
                push_piece(out, field);
                push_u32(out, v);
            }
            push_piece(out, ",\"fallback\":");
            push_piece(out, if l.fallback { "true" } else { "false" });
            push_piece(out, "}");
        }
    }
    push_piece(out, "}");
}

/// The four fields the configured and the live halves share, in wire order
/// and with no leading comma — written once so the two cannot drift.
#[cfg(feature = "panel")]
fn push_driver_fields(out: &mut String, planes: u8, clock_mhz: u8, chip: Chip, blank: u8) {
    push_piece(out, "\"planes\":");
    push_u32(out, planes as u32);
    push_piece(out, ",\"clock_mhz\":");
    push_u32(out, clock_mhz as u32);
    push_piece(out, ",\"chip\":\"");
    push_piece(out, chip.as_str());
    push_piece(out, "\",\"blank\":");
    push_u32(out, blank as u32);
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
        // same wiring, bigger tile → live on a STRIP-built matrix, where the
        // grid is only a pixel count and a map (a HUB75 board sizes its DMA
        // framebuffer from it at boot instead — see the #525 test below)
        let e = parse("matrix 64 64 1 1 tl row 0 0", &cur, 1024, &big_limits()).unwrap();
        assert!(!e.reboot_required);
        // same tile, snaked → reboot
        let e = parse("matrix 32 32 1 1 tl row 1 0", &cur, 1024, &big_limits()).unwrap();
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
    fn a_proj_line_is_the_running_pattern_s_override_and_is_not_stored() {
        // Gitea #598: `proj` is ephemeral and `proj1d/2d/3d` are the stored
        // defaults, so a body carrying both moves exactly one of each and the
        // Layout that gets persisted never mentions the override.
        let cur = strip_layout();
        let e = parse("proj1d x\nproj y", &cur, 60, &strip_limits()).unwrap();
        assert_eq!(e.layout.proj.proj1d, ProjectionMode::X, "the default moved");
        assert_eq!(e.proj_now, Some(Some(ProjectionMode::Y)), "and the override");
        assert!(!e.reboot_required);
        assert!(!e.layout.to_wire(60, &proto_name).contains("\nproj y"));

        // `default`, and a bare `proj`, mean "no override"
        for body in ["proj default", "proj"] {
            let e = parse(body, &cur, 60, &strip_limits()).unwrap();
            assert_eq!(e.proj_now, Some(None), "body {body:?}");
            assert_eq!(e.layout, cur, "body {body:?} stores nothing");
        }
        // absent = the body said nothing about it
        assert_eq!(parse("proj1d y", &cur, 60, &strip_limits()).unwrap().proj_now, None);
        // and an unknown token is a line error, like every other verb's
        assert_eq!(parse("proj sideways", &cur, 60, &strip_limits()).unwrap_err().line, 1);
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
        l.driver = PanelDriver { planes: 6, clock_mhz: 20, chip: Chip::Dp3246, blank: 4 };
        let wire = l.to_wire(6144, &proto_name);
        assert_eq!(
            wire,
            "matrix 64 32 3 1 br col 1 0 16 \npanel 6 20 dp3246 4\n\
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
        v.panel = Some(PanelView { est_hz: 57, drive: 1, driver_live: None });
        let mut s = String::new();
        l.push_json(&mut s, &v);
        assert!(s.contains("\"rot180\":0,\"scan\":0,\"est_hz\":57,\"drive\":1}"), "{s}");
        assert!(s.contains("\"w\":128,\"h\":64"));
        assert!(s.contains("\"count\":2"), "one implicit output covers both panels");
    }

    // --- how the panel is DRIVEN: the `panel` line (#525) --------------------

    /// A HUB75 board with a 64×64 panel stored, driver at the board default.
    fn panel_cur() -> Layout {
        Layout::board_default(LayoutKind::Matrix, Matrix::single(64, 64))
    }

    #[test]
    fn the_panel_line_round_trips() {
        let cur = panel_cur();
        let e = parse("panel 5 24 fm6126a 0", &cur, 4096, &panel_limits()).unwrap();
        assert!(e.driver_set);
        assert_eq!(
            e.layout.driver,
            PanelDriver { planes: 5, clock_mhz: 24, chip: Chip::Fm6126a, blank: 0 }
        );
        assert_eq!(e.layout.driver.clock_hz(), 24_000_000);
        assert_eq!(e.layout.driver.latch_clocks(), 1);
        // the stored record IS the wire, so it reads back byte-for-byte
        let wire = e.layout.to_wire(4096, &proto_name);
        assert!(wire.contains("\npanel 5 24 fm6126a 0"), "{wire}");
        let load = Limits { strict: false, ..panel_limits() };
        assert_eq!(parse(&wire, &panel_cur(), 4096, &load).unwrap().layout, e.layout);
        // only the DP3246 holds the latch longer than one clock
        let e = parse("panel 8 40 dp3246 8", &cur, 4096, &panel_limits()).unwrap();
        assert_eq!(e.layout.driver.latch_clocks(), 3);
        assert_eq!(e.layout.driver.clock_hz(), 40_000_000);
        // a strip Layout has no panel, so its stored form carries no line
        assert!(!strip_layout().to_wire(60, &proto_name).contains("panel"));
    }

    #[test]
    fn the_driver_defaults_when_absent_and_a_post_without_it_merges() {
        // a fresh Layout is exactly what every HUB75 image shipped before the
        // line existed
        assert_eq!(
            panel_cur().driver,
            PanelDriver { planes: 7, clock_mhz: 30, chip: Chip::ShiftReg, blank: 1 }
        );
        assert_eq!(panel_cur().driver.clock_hz(), 30_000_000);
        // …and a body that says nothing about the driver KEEPS the stored one
        let mut cur = panel_cur();
        cur.driver = PanelDriver { planes: 4, clock_mhz: 12, chip: Chip::Icn2038s, blank: 3 };
        let e = parse("matrix 64 64 1 1 tl row 0 0", &cur, 4096, &panel_limits()).unwrap();
        assert!(!e.driver_set, "the body said nothing about it");
        assert_eq!(e.layout.driver, cur.driver, "merge, not reset");
    }

    #[test]
    fn a_bad_panel_line_names_the_field_it_rejected() {
        let cur = panel_cur();
        let cases: [(&str, &str); 9] = [
            ("panel 3 30 shiftreg 1", "planes"),
            ("panel 9 30 shiftreg 1", "planes"),
            ("panel 7 1 shiftreg 1", "clock_mhz"),
            ("panel 7 41 shiftreg 1", "clock_mhz"),
            ("panel 7 30 fm6124 1", "chip"),
            ("panel 7 30 shiftreg 9", "blank"),
            ("panel 7 30 shiftreg", "expected"),
            ("panel", "expected"),
            ("panel x 30 shiftreg 1", "expected"),
        ];
        for (body, want) in cases {
            let e = parse(body, &cur, 4096, &panel_limits()).unwrap_err();
            assert_eq!(e.line, 1, "body {body:?}");
            assert!(e.msg.contains(want), "body {body:?} said {:?}", e.msg);
        }
        // at most one per body, like the kind line
        let two = "panel 7 30 shiftreg 1\npanel 6 30 shiftreg 1";
        assert_eq!(parse(two, &cur, 4096, &panel_limits()).unwrap_err().line, 2);
        // and both ends of every range ARE legal
        for body in ["panel 4 2 shiftreg 0", "panel 8 40 dp3246 8"] {
            assert!(parse(body, &cur, 4096, &panel_limits()).is_ok(), "body {body:?}");
        }
    }

    #[test]
    fn changing_the_driver_needs_a_reboot() {
        let cur = panel_cur();
        // all four fields are read once, when the DMA is set up
        for body in [
            "panel 6 30 shiftreg 1",
            "panel 7 20 shiftreg 1",
            "panel 7 30 fm6126a 1",
            "panel 7 30 shiftreg 0",
        ] {
            assert!(parse(body, &cur, 4096, &panel_limits()).unwrap().reboot_required, "{body}");
        }
        // restating the stored driver rebuilds nothing, so a Settings page may
        // POST the whole block on every edit
        assert!(!parse("panel 7 30 shiftreg 1", &cur, 4096, &panel_limits())
            .unwrap()
            .reboot_required);
        // and a driver written down on a STRIP Layout rebuilds nothing either
        let e = parse("panel 4 2 dp3246 8", &strip_layout(), 60, &strip_limits()).unwrap();
        assert!(!e.reboot_required, "a strip has no panel to rebuild");
    }

    #[test]
    fn a_panel_boards_framebuffer_geometry_is_reboot_required() {
        let cur = panel_cur();
        // the DMA framebuffer is allocated from the arrangement at boot…
        let smaller = "matrix 32 64 1 1 tl row 0 0";
        let e = parse(smaller, &cur, 4096, &panel_limits()).unwrap();
        assert!(e.reboot_required);
        assert!(cur.fb_geometry_changed(&e.layout));
        // …while the same edit on a strip-built matrix only resizes the grid
        assert!(!parse(smaller, &cur, 4096, &big_limits()).unwrap().reboot_required);
        // an unchanged arrangement rebuilds nothing on either
        let same = "matrix 64 64 1 1 tl row 0 0";
        assert!(!parse(same, &cur, 4096, &panel_limits()).unwrap().reboot_required);
        assert!(!cur.fb_geometry_changed(&cur));
        // scan stripes the framebuffer, so it is in both answers already
        assert!(parse("matrix 64 64 1 1 tl row 0 0 16", &cur, 4096, &panel_limits())
            .unwrap()
            .reboot_required);
    }

    #[test]
    fn scan_must_divide_half_the_panel_height() {
        let cur = panel_cur();
        // a 64-row panel is 32 address rows: 32 is the whole depth, anything
        // that tiles it stripes the framebuffer, 0 means "the board's own"
        for body in [
            "matrix 64 64 1 1 tl row 0 0",
            "matrix 64 64 1 1 tl row 0 0 1",
            "matrix 64 64 1 1 tl row 0 0 2",
            "matrix 64 64 1 1 tl row 0 0 8",
            "matrix 64 64 1 1 tl row 0 0 16",
            "matrix 64 64 1 1 tl row 0 0 32",
        ] {
            assert!(parse(body, &cur, 4096, &panel_limits()).is_ok(), "body {body:?}");
        }
        for body in [
            "matrix 64 64 1 1 tl row 0 0 3",
            "matrix 64 64 1 1 tl row 0 0 24",
            "matrix 64 64 1 1 tl row 0 0 48",
            "matrix 64 64 1 1 tl row 0 0 64",
        ] {
            let e = parse(body, &cur, 4096, &panel_limits()).unwrap_err();
            assert_eq!((e.line, e.msg), (1, "scan must divide ph/2"), "body {body:?}");
        }
        // an odd `ph` is not a shape a HUB75 driver can express at all…
        let e = parse("matrix 64 31 1 1 tl row 0 0", &cur, 4096, &panel_limits()).unwrap_err();
        assert!(e.msg.contains("ph must be even"), "{}", e.msg);
        // …but a strip-built matrix may be 64x31, as long as it claims no scan
        let strip = strip_layout();
        assert!(parse("matrix 64 31 1 1 tl row 0 0", &strip, 60, &big_limits()).is_ok());
        let e = parse("matrix 64 31 1 1 tl row 0 0 8", &strip, 60, &big_limits()).unwrap_err();
        assert_eq!(e.msg, "scan must divide ph/2");
    }

    #[test]
    fn chip_names_round_trip() {
        for c in Chip::ALL {
            assert_eq!(Chip::parse(c.as_str()), Some(c));
        }
        assert_eq!(Chip::ALL.len(), 4, "the `chips` list a UI offers");
        assert_eq!(Chip::parse("SHIFTREG"), None, "the wire is lower case");
        assert_eq!(Chip::parse(""), None);
    }

    #[test]
    fn a_host_with_no_panel_emits_no_driver_block() {
        let l = panel_cur();
        let mut s = String::new();
        l.push_json(&mut s, &view(&l, 4096, "{}"));
        assert!(!s.contains("\"driver\""), "{s}");
    }

    /// The `driver` block, configured + `chips` + live (#525).
    #[cfg(feature = "panel")]
    #[test]
    fn the_driver_block_reports_configured_chips_and_live() {
        let mut l = panel_cur();
        l.driver = PanelDriver { planes: 6, clock_mhz: 20, chip: Chip::Fm6126a, blank: 2 };
        let mut v = view(&l, 4096, "null");
        v.panel = Some(PanelView { est_hz: 57, drive: 1, driver_live: None });
        let mut s = String::new();
        l.push_json(&mut s, &v);
        assert!(
            s.contains(
                "\"driver\":{\"planes\":6,\"clock_mhz\":20,\"chip\":\"fm6126a\",\"blank\":2,\
                 \"chips\":[\"shiftreg\",\"fm6126a\",\"icn2038s\",\"dp3246\"],\"live\":null}"
            ),
            "{s}"
        );

        // the live half is what a UI diffs the configured one against
        v.panel = Some(PanelView {
            est_hz: 57,
            drive: 1,
            driver_live: Some(LiveDriver {
                planes: 7,
                clock_mhz: 30,
                chip: Chip::ShiftReg,
                blank: 1,
                w: 64,
                h: 64,
                scan: 32,
                fb_bytes: 28672,
                fallback: true,
            }),
        });
        let mut s = String::new();
        l.push_json(&mut s, &v);
        assert!(
            s.contains(
                "\"live\":{\"planes\":7,\"clock_mhz\":30,\"chip\":\"shiftreg\",\"blank\":1,\
                 \"w\":64,\"h\":64,\"scan\":32,\"fb_bytes\":28672,\"fallback\":true}}"
            ),
            "{s}"
        );
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
