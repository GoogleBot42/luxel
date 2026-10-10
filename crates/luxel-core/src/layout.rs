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
//! matrix  pw ph cols rows start dir snake rot [scan]     ← rot = <even>/<odd> degrees, or the legacy 0..3 mask
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
/// `rows` tile them; `start`/`dir`/`snake`/`rot` describe how the chain
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
    /// How the tiles are MOUNTED, in quarter turns clockwise (0..3 = 0°, 90°,
    /// 180°, 270°): `rot[0]` for the even chain lines — line 0 and so the
    /// FIRST panel — and `rot[1]` for the odd lines, the return legs of a
    /// serpentine wall. The driver assumes an upright tile (its first pixel
    /// at the tile's top-left); anything else is stated here, per line
    /// (Gitea #917: the first panel hung the other way, or a quarter turn
    /// so the ribbon runs down a column). A quarter turn needs a square
    /// tile. On the wire the field is `<even>/<odd>` in degrees, or the
    /// legacy `0..3` 180°-only mask every wall stored before this.
    pub rot: [u8; 2],
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
            rot: [0, 0],
            scan: 0,
        }
    }

    /// Quarter turns clockwise the tiles on chain line `line` are mounted
    /// at: `rot[0]` for the even lines, `rot[1]` for the odd ones (#917).
    pub const fn turns(&self, line: usize) -> u8 {
        self.rot[line % 2] & 3
    }

    /// Does any line hold a quarter turn (90° / 270°)? Those need `pw == ph`.
    pub const fn quarter_turned(&self) -> bool {
        self.rot[0] % 2 == 1 || self.rot[1] % 2 == 1
    }

    /// The wire form of `rot`: the legacy `0..3` mask when only 180° turns
    /// are involved (byte-identical to every stored wire before #917 — and
    /// what firmware from before it still parses), else `<even>/<odd>` in
    /// degrees.
    fn push_rot_wire(&self, out: &mut String) {
        if !self.quarter_turned() {
            let mask = u32::from(self.rot[1] == 2) | (u32::from(self.rot[0] == 2) << 1);
            push_u32(out, mask);
        } else {
            push_u32(out, u32::from(self.rot[0]) * 90);
            out.push('/');
            push_u32(out, u32::from(self.rot[1]) * 90);
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
    const fn wiring(&self) -> (u8, u8, u8, u8, bool, [u8; 2], u8) {
        (
            self.cols,
            self.rows,
            self.start as u8,
            self.dir as u8,
            self.snake,
            self.rot,
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

    /// Clocks the latch is held HIGH at the end of a row block — 1 for a
    /// shift register, 3 for a DP3246. On the chip rather than on
    /// [`PanelDriver`] so a host holding only a `live` reading (whose chip is
    /// all it kept) can work out the same OE window.
    pub const fn latch_clocks(self) -> u8 {
        match self {
            Chip::Dp3246 => 3,
            _ => 1,
        }
    }
}

/// How a HUB75 panel is DRIVEN, as opposed to how it is arranged: the BCM
/// bit depth, the LCD_CAM pixel clock, the driver chip's init and how long
/// OE is held off around the latch (Gitea #401 + #525, the `panel` wire
/// line). Every field was a compile-time constant before this. Three of the
/// four are read at boot, so changing them is `reboot_required`; `blank` is
/// applied live — see [`PanelDriver::boot_differs`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PanelDriver {
    /// BCM bitplanes, 4..=8. Fewer is a faster rescan and a coarser ramp:
    /// one rescan shifts the whole chain `2^planes - 1` times.
    pub planes: u8,
    /// LCD_CAM pixel clock in MHz — one of [`PanelDriver::CLOCKS`], not a
    /// free number (Gitea #771). Two reasons, both found the hard way: 40 MHz
    /// is out of spec for every driver chip we know and mis-samples an FM6124
    /// panel (the bench table in `firmware/src/hub75.rs`), and a rate that is
    /// not an exact integer divide of an LCD_CAM clock source is synthesised
    /// by esp-hal's FRACTIONAL divider, which dithers the period rather than
    /// producing an even clock.
    pub clock_mhz: u8,
    pub chip: Chip,
    /// Clocks at the START of every row block, and again just before the
    /// latch word, where OE is OFF — 0..=8. 1 is the stock template.
    ///
    /// The one panel setting that applies LIVE (Gitea #778): it is control
    /// bits the packer never writes, so a host re-formats its framebuffers
    /// in place. Raising it trades a little brightness for less ghosting
    /// between address rows — a knob you tune while watching the panel.
    pub blank: u8,
    /// On-time of the LEAST significant bitplane, in pixel clocks — the
    /// refresh lever (Gitea #460 / #789). **0 = full**: every plane
    /// is lit for the whole row block and re-shifted `2^k` times, the stock
    /// BCM schedule. A smaller value lights plane `k` for exactly `lsb · 2^k`
    /// clocks instead; the planes whose on-time is shorter than a row shift
    /// are then emitted ONCE with OE cut off early, so a rescan costs
    /// `E = t + 2^(planes − t) − 1` row shifts instead of `2^planes − 1` (`t`
    /// = the number of such planes). Brightness relative to stock is
    /// `lsb · (2^planes − 1) / (W · E)` — on-time per unit time, the pass
    /// having shortened too — so at `lsb = W >> t` the refresh doubles per
    /// step for a few percent of brightness (`W` = the lit clocks of a row
    /// block, `cols − latch − 2·blank`). Binary weights stay exact. Clamped to `W`
    /// at boot; read at boot (the DMA descriptor chain is built from it), so
    /// `reboot_required`. `live.lsb` reports the effective value.
    pub lsb: u16,
    /// Milliseconds of slack the RING driver (Gitea #857, firmware feature
    /// `hub75-ring`) sizes its slot ring for: how long the beam can run
    /// ahead of the packer before a row pair is late. The ring is sized in
    /// time, not rows — the row count follows from the schedule and the
    /// chain width (`luxel_hub75::ring::slots_for_slack`) and `live.ring_rows`
    /// reports it. 1..=50; default 3 while core 0 packs alone. Read at boot
    /// (the ring is allocated from it), so `reboot_required`; ignored by the
    /// two-buffer driver.
    pub ring_ms: u16,
}

impl Default for PanelDriver {
    /// The board default every HUB75 image shipped before the `panel` line
    /// existed: 7 planes, 30 MHz, no chip init, one blanking clock, full
    /// on-time.
    fn default() -> PanelDriver {
        PanelDriver { planes: 7, clock_mhz: 30, chip: Chip::ShiftReg, blank: 1, lsb: 0, ring_ms: 3 }
    }
}

impl PanelDriver {
    /// The pixel clocks a panel board offers, ascending — what
    /// `GET /api/layout` reports as `driver.clocks` so a UI never hard-codes
    /// the list, and the only values [`parse`] accepts on a `panel` line
    /// (Gitea #771).
    ///
    /// Derived from the hardware, not chosen for roundness. esp-hal's i8080
    /// driver doubles the requested rate (the S3 errata puts the LCD_PCLK
    /// divider at ≥ 2) and then divides an LCD_CAM source down, so the real
    /// pixel clock is `source / (2 · N)`; the sources on an S3 are XTAL
    /// (40 MHz) and PLL_D2 (PLL 480 / 2 = 240 MHz, and the S3's PLL is 480 at
    /// every `CpuClock`). The rates with an exact INTEGER `N` are therefore
    /// 120/N and 20/N MHz: 30, 24, 20, 15, 12, 10, 8, 6, 5, 4, 3, 2. Anything
    /// else — 16 and 25 MHz included — comes out of esp-hal's fractional
    /// divider, which dithers the clock period instead of dividing evenly.
    ///
    /// The list is that set capped at 30 (the FM6124 datasheet ceiling; 40 MHz
    /// visibly split the bench panel) and floored at 8, below which a 7-plane
    /// 64×64 rescan falls under ~31 Hz and flickers.
    pub const CLOCKS: [u8; 7] = [8, 10, 12, 15, 20, 24, 30];

    /// The `clock_mhz` error message, spelled out so a UI that has not read
    /// `driver.clocks` still tells the user the whole list.
    const CLOCKS_MSG: &'static str = "panel: clock_mhz must be one of 8|10|12|15|20|24|30";

    pub const fn clock_hz(&self) -> u32 {
        self.clock_mhz as u32 * 1_000_000
    }

    /// Whether `mhz` is one of [`PanelDriver::CLOCKS`].
    pub fn clock_supported(mhz: u8) -> bool {
        PanelDriver::CLOCKS.contains(&mhz)
    }

    /// Clocks the latch is held HIGH at the end of a row block — 1 for a
    /// shift register, 3 for a DP3246. See [`Chip::latch_clocks`].
    pub const fn latch_clocks(&self) -> u8 {
        self.chip.latch_clocks()
    }

    /// Whether two drivers differ in a field a BOOT builds.
    ///
    /// `planes` sizes the DMA framebuffer, `clock_mhz` is the LCD_CAM's and
    /// `chip` is a register init bit-banged on the pins before the DMA
    /// starts — all three are set up once and are `reboot_required`.
    ///
    /// **`blank` is not** (Gitea #778). It is nothing but control bits in the
    /// framebuffer words — the OE window and the latch tail
    /// [`crate::layout`]'s consumers write with `luxel_hub75::format` — and
    /// the packer never touches those, so a host can re-`format` each buffer
    /// in place between frames and the new blanking is on the panel within a
    /// frame. It is the one panel knob a user tunes by LOOKING at the panel
    /// (ghosting between address rows), which is exactly the knob a reboot
    /// per attempt makes unusable.
    pub fn boot_differs(&self, other: &PanelDriver) -> bool {
        (self.planes, self.clock_mhz, self.chip, self.lsb, self.ring_ms)
            != (other.planes, other.clock_mhz, other.chip, other.lsb, other.ring_ms)
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

/// One panel of a HUB75 chain, as the wall has it (Gitea #920): which grid
/// cell it fills and how it is mounted. A chain is a list of these in ribbon
/// order — index 0 is the panel the ribbon enters.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Tile {
    /// Grid cell, 0-based from the top-left, `cx < cols`, `cy < rows`.
    pub cx: u8,
    pub cy: u8,
    /// Quarter turns clockwise from upright (0..3), as [`Matrix::rot`].
    pub turns: u8,
}

impl Matrix {
    /// Chain position `p` under the RULE fields (`start`/`dir`/`snake`/`rot`)
    /// — the regular-pattern generator, which is what every wall was
    /// described by before an explicit `chain` line existed. `None` past the
    /// end of the chain.
    #[must_use]
    pub fn rule_tile(&self, p: usize) -> Option<Tile> {
        let (cols, rows) = (self.cols as usize, self.rows as usize);
        if p >= cols * rows || cols == 0 || rows == 0 {
            return None;
        }
        // Which corner line 0 starts at, as a pair of axis flips.
        let flip_x = matches!(self.start, Corner::Tr | Corner::Br);
        let flip_y = matches!(self.start, Corner::Bl | Corner::Br);
        // `line` walks across the lines, `k` walks along one.
        let run = match self.dir {
            RunDir::Row => cols,
            RunDir::Col => rows,
        };
        let (line, mut k) = (p / run, p % run);
        // A snaked chain comes back the other way along every odd line.
        if self.snake && line % 2 == 1 {
            k = run - 1 - k;
        }
        let (cx, cy) = match self.dir {
            RunDir::Row => (k, line),
            RunDir::Col => (line, k),
        };
        let cx = if flip_x { cols - 1 - cx } else { cx };
        let cy = if flip_y { rows - 1 - cy } else { cy };
        Some(Tile { cx: cx as u8, cy: cy as u8, turns: self.turns(line) })
    }

    /// Every chain position under the rule, in ribbon order.
    pub fn rule_tiles(&self) -> Vec<Tile> {
        (0..self.panels() as usize).filter_map(|p| self.rule_tile(p)).collect()
    }
}

/// The whole configured Layout, minus the pixel count and the map payload —
/// see the module docs for why those stay where they already live.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Layout {
    pub kind: LayoutKind,
    /// Meaningful when `kind` is [`LayoutKind::Matrix`]; carried across a
    /// kind change so switching to Strip and back does not lose the wiring.
    pub matrix: Matrix,
    /// The chain as an explicit per-panel list (the `chain` wire line, Gitea
    /// #920) — what the arrangement editor writes when the wall is
    /// transcribed panel by panel. Empty = derive it from the rule fields of
    /// `matrix`, which is every Layout stored before this existed. When
    /// present it is exactly `cols × rows` entries, each cell once.
    pub chain: Vec<Tile>,
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
            chain: Vec::new(),
            outputs: Vec::new(),
            proj: Projection::DEFAULT,
            driver: PanelDriver::default(),
        }
    }

    /// The chain as the remap builder walks it, in ribbon order: the explicit
    /// list when there is one, else the rule's (Gitea #920).
    pub fn tiles(&self) -> Vec<Tile> {
        if self.chain.is_empty() {
            self.matrix.rule_tiles()
        } else {
            self.chain.clone()
        }
    }

    /// Does the arrangement — which panel sits where and how it is turned —
    /// differ between the two, however each one spells it?
    pub fn arrangement_differs(&self, next: &Layout) -> bool {
        self.tiles() != next.tiles()
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
                // the `panel` line: three of its four fields are read once,
                // at boot. `blank` applies live — see
                // [`PanelDriver::boot_differs`] (Gitea #778).
                || self.driver.boot_differs(&next.driver))
        {
            return true;
        }
        // The arrangement — which panel sits where, how it is turned — is a
        // remap TABLE: on a panel board the driver swaps it between frames
        // (Gitea #920), so only the framebuffer's shape needs a boot there.
        // A strip-built matrix builds no table at all; its wiring fields were
        // reboot-required since #475 and stay so.
        if next.kind == LayoutKind::Matrix
            && !panel_board
            && (self.kind != LayoutKind::Matrix || self.matrix.wiring() != next.matrix.wiring())
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
                push_u32(&mut out, u32::from(m.snake));
                out.push(' ');
                m.push_rot_wire(&mut out);
                out.push(' ');
                push_u32(&mut out, m.scan as u32);
                out.push(' ');
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
                // The fifth field is optional on the way IN (a stored body
                // from before #789 has four) but always written on the way
                // out, so the persisted text is unambiguous.
                out.push(' ');
                push_u32(&mut out, d.lsb as u32);
                // Likewise the sixth (Gitea #857): optional in, always out.
                out.push(' ');
                push_u32(&mut out, d.ring_ms as u32);
                // The explicit chain (Gitea #920), only when there is one —
                // a rule-described wall stays byte-identical to what it was.
                if !self.chain.is_empty() {
                    push_piece(&mut out, "\nchain");
                    for t in &self.chain {
                        out.push(' ');
                        push_tile_wire(&mut out, t);
                    }
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
/// matrix <pw> <ph> <cols> <rows> <start> <dir> <snake> <rot> [<scan>]
/// map [grid <w> <h> | <dims> <raw16.16…>]
/// panel <planes> <clock_mhz> <chip> <blank> [<lsb>]
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
    // `Some` = the body spoke about the chain: a list, or `chain` alone to
    // clear it. `None` = it did not, and the rules below decide.
    let mut chain: Option<Vec<Tile>> = None;

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
                    "expected: matrix <pw> <ph> <cols> <rows> <tl|tr|bl|br> <row|col> <0|1> <rot> [scan]",
                ))?;
                if m.quarter_turned() && m.pw != m.ph {
                    return Err(err("a tile turned 90° must be square (pw == ph)"));
                }
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
            // The chain as the wall has it (Gitea #920): every panel in
            // ribbon order, `cx,cy,deg`. Validated against the matrix line
            // in force once the whole body is read (it may come first).
            "chain" => {
                if chain.is_some() {
                    return Err(err("only one chain line per body"));
                }
                let mut list = Vec::new();
                for tok in it.by_ref() {
                    let t = parse_tile(tok)
                        .ok_or(err("expected: chain <cx>,<cy>,<0|90|180|270> … (one per panel, in ribbon order)"))?;
                    if list.len() >= 255 * 255 {
                        return Err(err("chain is longer than any arrangement"));
                    }
                    list.push(t);
                }
                chain = Some(list);
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
                    "unknown line (want strip|matrix|map|panel|chain|out|proj|proj1d|proj2d|proj3d)",
                ))
            }
        }
    }

    // The explicit chain (Gitea #920). A body that lists one must list the
    // whole arrangement, each cell exactly once; a bare `chain` clears it;
    // and a body that re-describes the wall by RULE (a matrix line whose
    // arrangement fields moved) or changes its tiling drops the list, so
    // the rule the user just edited is what shows — the editor writes the
    // list back the moment they touch a panel.
    match chain {
        Some(list) if list.is_empty() => next.chain.clear(),
        Some(list) => {
            let m = &next.matrix;
            if list.len() != m.panels() as usize {
                return Err(LayoutError { line: 0, msg: "chain must name every panel exactly once (cols*rows entries)" });
            }
            let mut seen = Vec::new();
            seen.resize(list.len(), false);
            for t in &list {
                if t.cx >= m.cols || t.cy >= m.rows {
                    return Err(LayoutError { line: 0, msg: "chain names a cell outside the arrangement" });
                }
                let i = t.cy as usize * m.cols as usize + t.cx as usize;
                if seen[i] {
                    return Err(LayoutError { line: 0, msg: "chain names a cell twice" });
                }
                seen[i] = true;
                if t.turns % 2 == 1 && m.pw != m.ph {
                    return Err(LayoutError { line: 0, msg: "a tile turned 90° must be square (pw == ph)" });
                }
            }
            next.chain = list;
        }
        None => {
            let g = |m: &Matrix| (m.cols, m.rows);
            if g(&cur.matrix) != g(&next.matrix) || cur.matrix.wiring() != next.matrix.wiring() {
                next.chain.clear();
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
        "expected: panel <planes> <clock_mhz> <shiftreg|fm6126a|icn2038s|dp3246> <blank> [<lsb>] [<ring_ms>]";
    let planes = it.next().and_then(num).ok_or(USAGE)?;
    if !(4..=8).contains(&planes) {
        return Err("panel: planes must be 4..8");
    }
    let clock_mhz = it.next().and_then(num).ok_or(USAGE)?;
    // A FIXED LIST, not a range (Gitea #771): Jeremy set 40 MHz "to see what
    // happens" on 2026-09-26 and got a mis-sampling panel, and the values in
    // between are not all reachable with an even clock anyway — see
    // [`PanelDriver::CLOCKS`].
    if u8::try_from(clock_mhz).map(PanelDriver::clock_supported) != Ok(true) {
        return Err(PanelDriver::CLOCKS_MSG);
    }
    let chip = Chip::parse(it.next().ok_or(USAGE)?)
        .ok_or("panel: chip must be shiftreg|fm6126a|icn2038s|dp3246")?;
    let blank = it.next().and_then(num).ok_or(USAGE)?;
    if blank > 8 {
        return Err("panel: blank must be 0..8");
    }
    // The fifth field is OPTIONAL: a line written before it existed (or by a
    // console that does not know it) keeps the full schedule, which is what
    // every stored Layout meant until now.
    let lsb = match it.next() {
        None => 0,
        Some(s) => {
            let v = num(s).ok_or("panel: lsb must be 0..65535 (0 = full on-time)")?;
            if v > u16::MAX as u32 {
                return Err("panel: lsb must be 0..65535 (0 = full on-time)");
            }
            v
        }
    };
    // The sixth field is optional the same way (Gitea #857): a line without
    // it keeps the default slack.
    let ring_ms = match it.next() {
        None => PanelDriver::default().ring_ms as u32,
        Some(s) => {
            let v = num(s).ok_or("panel: ring_ms must be 1..50")?;
            if !(1..=50).contains(&v) {
                return Err("panel: ring_ms must be 1..50");
            }
            v
        }
    };
    Ok(PanelDriver {
        planes: planes as u8,
        clock_mhz: clock_mhz as u8,
        chip,
        blank: blank as u8,
        lsb: lsb as u16,
        ring_ms: ring_ms as u16,
    })
}

fn parse_matrix<'a>(it: &mut impl Iterator<Item = &'a str>) -> Option<Matrix> {
    let pw = u16::try_from(num(it.next()?)?).ok()?;
    let ph = u16::try_from(num(it.next()?)?).ok()?;
    let cols = u8::try_from(num(it.next()?)?).ok()?;
    let rows = u8::try_from(num(it.next()?)?).ok()?;
    let start = Corner::from_str(it.next()?)?;
    let dir = RunDir::from_str(it.next()?)?;
    let snake = flag(it.next()?)?;
    let rot = parse_rot(it.next()?)?;
    let scan = match it.next() {
        None => 0,
        Some(v) => u8::try_from(num(v)?).ok()?,
    };
    if pw == 0 || ph == 0 || cols == 0 || rows == 0 {
        return None;
    }
    Some(Matrix { pw, ph, cols, rows, start, dir, snake, rot, scan })
}

/// The `rot` field of a `matrix` line (Gitea #917): `<even>/<odd>` in
/// degrees, each `0|90|180|270` clockwise, for the even lines (the first
/// panel's) and the odd lines — or the legacy 180°-only mask `0..3` every
/// wall stored before this (bit 1 = odd lines, bit 2 = even lines), which
/// is also what a 180°-only arrangement is written back as.
/// One `chain` entry, `cx,cy,deg`.
fn parse_tile(s: &str) -> Option<Tile> {
    let mut it = s.split(',');
    let cx = u8::try_from(num(it.next()?)?).ok()?;
    let cy = u8::try_from(num(it.next()?)?).ok()?;
    let deg = num(it.next()?)?;
    if it.next().is_some() || deg % 90 != 0 || deg > 270 {
        return None;
    }
    Some(Tile { cx, cy, turns: (deg / 90) as u8 })
}

fn push_tile_wire(out: &mut String, t: &Tile) {
    push_u32(out, u32::from(t.cx));
    out.push(',');
    push_u32(out, u32::from(t.cy));
    out.push(',');
    push_u32(out, u32::from(t.turns & 3) * 90);
}

fn parse_rot(s: &str) -> Option<[u8; 2]> {
    if let Some((e, o)) = s.split_once('/') {
        let deg = |t: &str| -> Option<u8> {
            let d = num(t)?;
            if d % 90 != 0 || d > 270 {
                return None;
            }
            Some((d / 90) as u8)
        };
        return Some([deg(e)?, deg(o)?]);
    }
    let mask = num(s)?;
    if mask > 3 {
        return None;
    }
    Some([if mask & 2 != 0 { 2 } else { 0 }, if mask & 1 != 0 { 2 } else { 0 }])
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
    /// A boot self-heal this host performed (#822): the stored shape left the
    /// heap under the runtime floor and was reverted to the board default.
    /// `None` — and the field absent from the JSON — when nothing was
    /// reverted, which is every ordinary boot. NOT feature-gated: the
    /// self-heal is board-agnostic, so every host can report one.
    pub reverted: Option<Reverted>,
}

/// A boot self-heal the host performed (#822) — what it threw away and the
/// reading that made it.
///
/// Persisted by the host across the reboot that follows, so the console can
/// explain a shape that silently changed under it, and cleared by the next
/// successful `POST /api/layout`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Reverted {
    /// Pixels of the stored shape that was reverted.
    pub from_pixels: u32,
    /// Free heap at the end of that boot, bytes — under
    /// [`crate::budget::RUNTIME_FLOOR`], which is why the revert happened.
    pub heap_free: u32,
}

/// What a finished boot should do about the heap it ended up with (#822) —
/// the decision half of the self-heal, pure so it can be tested on the host.
///
/// The firmware calls this once, at the point the boot-loop guard decides the
/// image is healthy (~60 s in: WiFi up, web up, engine built), with the free
/// heap it measures there. Everything but [`Heal::Revert`] means "log it and
/// carry on" — a starved board that cannot be improved must not reboot, or
/// the self-heal becomes the boot loop it exists to prevent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Heal {
    /// The heap is at or above the floor. Nothing to do.
    Healthy,
    /// Under the floor, but the stored shape already IS the board default —
    /// there is nothing smaller to fall back to. This is the board being too
    /// small for what is running on it, not a bad stored layout.
    AtDefault,
    /// Under the floor, and the board default is no smaller than what is
    /// stored: reverting would cost the user their configuration and buy no
    /// memory.
    NoSmaller,
    /// Under the floor, and this exact shape has been reverted once already —
    /// the revert record survived but the new layout did not land (a refused
    /// flash write). Reverting again would reboot forever.
    AlreadyReverted,
    /// Revert to the board default and reboot once.
    Revert,
}

/// [`Heal`] for one boot.
///
/// * `free` / `floor` — measured free heap against [`crate::budget::RUNTIME_FLOOR`].
/// * `stored_pixels` / `default_pixels` — the pixel extent of the stored shape
///   and of the board default's. A revert only happens when the default is
///   strictly smaller, which is what makes this safe to run on a strip board
///   too: there the two are equal unless a `matrix` line set the count.
/// * `at_default` — the stored shape IS the board default (kind and matrix).
/// * `reverted_from` — `from_pixels` of the stored revert record, `0` for none.
#[must_use]
pub fn heal_decision(
    free: usize,
    floor: usize,
    stored_pixels: u32,
    default_pixels: u32,
    at_default: bool,
    reverted_from: u32,
) -> Heal {
    if free >= floor {
        return Heal::Healthy;
    }
    if at_default {
        return Heal::AtDefault;
    }
    if default_pixels >= stored_pixels {
        return Heal::NoSmaller;
    }
    if reverted_from == stored_pixels {
        return Heal::AlreadyReverted;
    }
    Heal::Revert
}

/// [`heal_decision`] that also knows the LARGEST free block.
///
/// Total free heap is the wrong yardstick on its own: on 2026-09-27 a
/// Seengreat with a stored 2x1 on the two-buffer build sat ABOVE the 20 KB
/// floor while every route answered 503 — the heap was there, but so
/// fragmented that the web server could not get its 4 KB connection buffer
/// or an 8 KB status body. `largest` under `largest_floor` is that state,
/// and counts as starved exactly like `free` under `floor`.
#[must_use]
pub fn heal_decision_fragmented(
    free: usize,
    floor: usize,
    largest: usize,
    largest_floor: usize,
    stored_pixels: u32,
    default_pixels: u32,
    at_default: bool,
    reverted_from: u32,
) -> Heal {
    let free = if largest < largest_floor { 0 } else { free };
    heal_decision(free, floor, stored_pixels, default_pixels, at_default, reverted_from)
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
    /// The test card the panel is showing instead of the pattern (Gitea
    /// #920), `POST /api/layout/card`.
    pub card: Card,
}

/// What a panel board draws instead of the pattern while the wall is being
/// transcribed (Gitea #920). Not persisted; a boot is always `Off`.
#[cfg(feature = "panel")]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Card {
    #[default]
    Off,
    /// Every PHYSICAL panel shows its ribbon number and an arrow to its own
    /// top — drawn straight into the driver's blocks, remap bypassed. What
    /// the user reads off the wall into the editor.
    Panels,
    /// Every GRID cell shows its ribbon number and an up arrow, through the
    /// live remap: right when every panel shows its number upright.
    Cells,
}

#[cfg(feature = "panel")]
impl Card {
    pub const fn as_str(self) -> &'static str {
        match self {
            Card::Off => "off",
            Card::Panels => "panels",
            Card::Cells => "cells",
        }
    }
    pub fn from_str(s: &str) -> Option<Card> {
        match s {
            "off" => Some(Card::Off),
            "panels" => Some(Card::Panels),
            "cells" => Some(Card::Cells),
            _ => None,
        }
    }
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
    /// The EFFECTIVE LSB on-time in pixel clocks, never 0: the configured
    /// [`PanelDriver::lsb`] clamped to the lit width of a row block, or that
    /// width itself when the configured value is 0 (full).
    pub lsb: u16,
    /// The framebuffer's chain extent in pixels: `w` = `pw` × chain length,
    /// `h` = `ph`.
    pub w: u16,
    pub h: u16,
    /// Address rows the driver scans.
    pub scan: u16,
    /// Bytes of ONE framebuffer (there are two, double-buffered) — or, on
    /// the ring driver, of the whole slot ring.
    pub fb_bytes: u32,
    /// The configured geometry/driver did not fit in internal RAM and the
    /// firmware booted the board default instead.
    pub fallback: bool,
    /// The `ring_ms` the running driver booted with (the ring driver sized
    /// its ring from it; the two-buffer driver ignores it) — the value a
    /// client compares the configured one against.
    pub ring_ms: u16,
    /// Ring driver (Gitea #857): slots in the ring, 0 on the two-buffer
    /// driver.
    pub ring_rows: u16,
    /// Ring driver: the slack those slots buy, microseconds; 0 otherwise.
    pub ring_slack_us: u32,
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
            push_piece(out, ",\"rot\":[");
            push_u32(out, u32::from(self.matrix.rot[0]) * 90);
            out.push(',');
            push_u32(out, u32::from(self.matrix.rot[1]) * 90);
            push_piece(out, "]");
            push_piece(out, ",\"scan\":");
            push_u32(out, self.matrix.scan as u32);
            // The chain as the remap walks it (Gitea #920): every panel in
            // ribbon order as `[cx, cy, deg]`, whether it came from the rule
            // or from an explicit list — `explicit` says which.
            push_piece(out, ",\"tiles\":[");
            for (i, t) in self.tiles().iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('[');
                push_u32(out, u32::from(t.cx));
                out.push(',');
                push_u32(out, u32::from(t.cy));
                out.push(',');
                push_u32(out, u32::from(t.turns & 3) * 90);
                out.push(']');
            }
            push_piece(out, "],\"explicit\":");
            push_piece(out, if self.chain.is_empty() { "false" } else { "true" });
            #[cfg(feature = "panel")]
            if let Some(p) = v.panel {
                push_piece(out, ",\"est_hz\":");
                push_u32(out, p.est_hz);
                push_piece(out, ",\"drive\":");
                push_u32(out, p.drive);
                push_piece(out, ",\"card\":\"");
                push_piece(out, p.card.as_str());
                push_piece(out, "\"");
            }
            push_piece(out, "}");
        }
        // The driver belongs to the BOARD, not to the kind: a HUB75 board
        // with a coordinate map installed is still driving a panel.
        #[cfg(feature = "panel")]
        if let Some(p) = v.panel {
            push_driver_json(out, &self.driver, &p);
        }
        // Absent unless a boot actually reverted something (#822), so an
        // ordinary body is byte-identical to one written before this existed.
        if let Some(r) = v.reverted {
            push_piece(out, ",\"reverted\":{\"from_pixels\":");
            push_u32(out, r.from_pixels);
            push_piece(out, ",\"heap_free\":");
            push_u32(out, r.heap_free);
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

/// The `driver` block (#525): the CONFIGURED driver, the chip and pixel-clock
/// lists so a UI never hard-codes either, and what the firmware actually
/// booted — `null` there when the panel output is off entirely. Only a host
/// with a panel driver emits it, so a strip board's body is byte-identical to
/// one built before this existed.
#[cfg(feature = "panel")]
fn push_driver_json(out: &mut String, d: &PanelDriver, p: &PanelView) {
    push_piece(out, ",\"driver\":{");
    push_driver_fields(out, d.planes, d.clock_mhz, d.chip, d.blank, d.lsb, d.ring_ms);
    push_piece(out, ",\"chips\":[");
    for (i, c) in Chip::ALL.iter().enumerate() {
        push_piece(out, if i > 0 { ",\"" } else { "\"" });
        push_piece(out, c.as_str());
        push_piece(out, "\"");
    }
    // The clock is a fixed dropdown, not a number field (#771) — the list
    // rides beside `chips` for the same reason: the device decides it.
    push_piece(out, "],\"clocks\":[");
    for (i, c) in PanelDriver::CLOCKS.iter().enumerate() {
        if i > 0 {
            push_piece(out, ",");
        }
        push_u32(out, *c as u32);
    }
    push_piece(out, "],\"live\":");
    match &p.driver_live {
        None => push_piece(out, "null"),
        Some(l) => {
            push_piece(out, "{");
            // `ring_ms` in `live` is what the ring was sized FOR; `ring_rows`
            // and `ring_slack_us` are what it got (0 on the two-buffer driver).
            push_driver_fields(out, l.planes, l.clock_mhz, l.chip, l.blank, l.lsb, l.ring_ms);
            for (field, v) in [
                (",\"w\":", l.w as u32),
                (",\"h\":", l.h as u32),
                (",\"scan\":", l.scan as u32),
                (",\"fb_bytes\":", l.fb_bytes),
                (",\"ring_rows\":", l.ring_rows as u32),
                (",\"ring_slack_us\":", l.ring_slack_us),
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

/// The six fields the configured and the live halves share, in wire order
/// and with no leading comma — written once so the two cannot drift. `lsb`
/// is the CONFIGURED value in the `driver` block (0 = full) and the
/// EFFECTIVE one in `live` (never 0) — see [`LiveDriver::lsb`].
#[cfg(feature = "panel")]
fn push_driver_fields(
    out: &mut String,
    planes: u8,
    clock_mhz: u8,
    chip: Chip,
    blank: u8,
    lsb: u16,
    ring_ms: u16,
) {
    push_piece(out, "\"planes\":");
    push_u32(out, planes as u32);
    push_piece(out, ",\"clock_mhz\":");
    push_u32(out, clock_mhz as u32);
    push_piece(out, ",\"chip\":\"");
    push_piece(out, chip.as_str());
    push_piece(out, "\",\"blank\":");
    push_u32(out, blank as u32);
    push_piece(out, ",\"lsb\":");
    push_u32(out, lsb as u32);
    push_piece(out, ",\"ring_ms\":");
    push_u32(out, ring_ms as u32);
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
            reverted: None,
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


    /// The `rot` field (Gitea #917): the legacy `0..3` 180°-only mask still
    /// parses and is written back as itself, `<even>/<odd>` degrees carry
    /// quarter turns, and anything else is refused.
    #[test]
    fn rot_is_degrees_per_line_or_the_legacy_mask() {
        let mut cur = strip_layout();
        cur.kind = LayoutKind::Matrix;
        cur.matrix = Matrix::single(32, 32);
        for (v, want) in [(0u8, [0u8, 0]), (1, [0, 2]), (2, [2, 0]), (3, [2, 2])] {
            let body = alloc::format!("matrix 32 32 2 2 tl row 1 {v}");
            let e = parse(&body, &cur, 4096, &big_limits()).unwrap();
            assert_eq!(e.layout.matrix.rot, want, "{body}");
            let wire = e.layout.to_wire(4096, &proto_name);
            assert!(wire.starts_with(&alloc::format!("matrix 32 32 2 2 tl row 1 {v} ")), "{wire}");
        }
        for (s, want, back) in [("90/270", [1u8, 3], "90/270"), ("0/180", [0, 2], "1"), ("270/0", [3, 0], "270/0"), ("180/180", [2, 2], "3")] {
            let body = alloc::format!("matrix 32 32 2 2 tl row 1 {s}");
            let e = parse(&body, &cur, 4096, &big_limits()).unwrap();
            assert_eq!(e.layout.matrix.rot, want, "{body}");
            let wire = e.layout.to_wire(4096, &proto_name);
            assert!(wire.starts_with(&alloc::format!("matrix 32 32 2 2 tl row 1 {back} ")), "{wire}");
        }
        for bad in ["4", "45/0", "0/360", "90", "90/", "/90"] {
            let body = alloc::format!("matrix 32 32 2 2 tl row 1 {bad}");
            assert!(parse(&body, &cur, 4096, &big_limits()).is_err(), "{body}");
        }
        // a quarter turn swaps a tile's axes, so the tile has to be square
        let e = parse("matrix 64 32 2 1 tl row 0 90/0", &cur, 4096, &big_limits());
        assert_eq!(e.unwrap_err().msg, "a tile turned 90° must be square (pw == ph)");
        assert!(parse("matrix 64 32 2 1 tl row 0 180/0", &cur, 4096, &big_limits()).is_ok());
    }

    /// The explicit chain (Gitea #920): a `chain` line names every panel in
    /// ribbon order, replaces the rule, round-trips, and is validated as a
    /// permutation of the cells.
    #[test]
    fn a_chain_line_describes_the_wall_panel_by_panel() {
        let mut cur = strip_layout();
        cur.kind = LayoutKind::Matrix;
        cur.matrix = Matrix::single(32, 32);
        let lim = big_limits();
        // the rule alone: tiles come from it, nothing explicit
        let e = parse("matrix 32 32 2 2 tr col 1 0", &cur, 4096, &lim).unwrap();
        assert!(e.layout.chain.is_empty());
        let rule: Vec<(u8, u8, u8)> = e.layout.tiles().iter().map(|t| (t.cx, t.cy, t.turns)).collect();
        assert_eq!(rule, vec![(1, 0, 0), (1, 1, 0), (0, 1, 0), (0, 0, 0)]);
        // Jeremy's wall, transcribed: same cells, quarter turns per panel
        let body = "matrix 32 32 2 2 tr col 1 0\nchain 1,0,90 1,1,90 0,1,270 0,0,270";
        let e = parse(body, &cur, 4096, &lim).unwrap();
        let got: Vec<(u8, u8, u8)> = e.layout.tiles().iter().map(|t| (t.cx, t.cy, t.turns)).collect();
        assert_eq!(got, vec![(1, 0, 1), (1, 1, 1), (0, 1, 3), (0, 0, 3)]);
        assert_eq!(e.layout.chain.len(), 4);
        let wire = e.layout.to_wire(4096, &proto_name);
        assert!(wire.contains("\nchain 1,0,90 1,1,90 0,1,270 0,0,270"), "{wire}");
        // …and the persisted form reads back to the same chain
        let back = parse(&wire, &cur, 4096, &lim).unwrap();
        assert_eq!(back.layout.chain, e.layout.chain);
        // a chain line may come first, and alone (the editor posts just it)
        let stored = e.layout.clone();
        let e2 = parse("chain 0,0,0 1,0,0 0,1,180 1,1,180", &stored, 4096, &lim).unwrap();
        assert_eq!(e2.layout.chain[2], Tile { cx: 0, cy: 1, turns: 2 });
        assert_eq!(e2.layout.matrix, stored.matrix, "the matrix line is untouched");
        // a bare `chain` clears it back to the rule
        let e3 = parse("chain", &stored, 4096, &lim).unwrap();
        assert!(e3.layout.chain.is_empty());
        // JSON: the effective tiles, and whether they are explicit
        let mut s = String::new();
        stored.push_json(&mut s, &view(&stored, 4096, "{}"));
        assert!(s.contains("\"tiles\":[[1,0,90],[1,1,90],[0,1,270],[0,0,270]],\"explicit\":true"), "{s}");
        let mut s = String::new();
        e3.layout.push_json(&mut s, &view(&e3.layout, 4096, "{}"));
        assert!(s.contains("\"tiles\":[[1,0,0],[1,1,0],[0,1,0],[0,0,0]],\"explicit\":false"), "{s}");
    }

    #[test]
    fn a_chain_must_be_a_permutation_of_the_cells() {
        let mut cur = strip_layout();
        cur.kind = LayoutKind::Matrix;
        cur.matrix = Matrix::single(32, 32);
        cur.matrix.cols = 2;
        cur.matrix.rows = 2;
        let lim = big_limits();
        let msg = |body: &str| parse(body, &cur, 4096, &lim).unwrap_err().msg;
        assert_eq!(msg("chain 0,0,0 1,0,0 0,1,0"), "chain must name every panel exactly once (cols*rows entries)");
        assert_eq!(msg("chain 0,0,0 1,0,0 0,1,0 2,1,0"), "chain names a cell outside the arrangement");
        assert_eq!(msg("chain 0,0,0 1,0,0 0,1,0 0,1,0"), "chain names a cell twice");
        assert!(msg("chain 0,0,45 1,0,0 0,1,0 1,1,0").starts_with("expected: chain"));
        assert!(msg("chain 0,0 1,0,0 0,1,0 1,1,0").starts_with("expected: chain"));
        assert_eq!(msg("chain 0,0,0 1,0,0\nchain 0,1,0 1,1,0"), "only one chain line per body");
        // a quarter turn needs a square tile, in the list as on the matrix line
        assert_eq!(
            msg("matrix 32 16 2 2 tl row 0 0\nchain 0,0,90 1,0,0 0,1,0 1,1,0"),
            "a tile turned 90° must be square (pw == ph)"
        );
    }

    /// Re-describing the wall by rule, or re-tiling it, drops the list — the
    /// thing the user just edited is what shows.
    #[test]
    fn a_rule_or_tiling_change_drops_the_explicit_chain() {
        let mut cur = strip_layout();
        cur.kind = LayoutKind::Matrix;
        cur.matrix = Matrix::single(32, 32);
        let lim = big_limits();
        let stored = parse("matrix 32 32 2 2 tr col 1 0\nchain 1,0,90 1,1,90 0,1,270 0,0,270", &cur, 4096, &lim)
            .unwrap()
            .layout;
        // same matrix line restated: the list survives
        let e = parse("matrix 32 32 2 2 tr col 1 0", &stored, 4096, &lim).unwrap();
        assert_eq!(e.layout.chain.len(), 4, "restating the rule keeps the transcription");
        // a `panel` line or projection alone: untouched too
        let e = parse("proj2d xy", &stored, 4096, &lim).unwrap();
        assert_eq!(e.layout.chain.len(), 4);
        // the rule moved: the list goes
        let e = parse("matrix 32 32 2 2 tl row 0 0", &stored, 4096, &lim).unwrap();
        assert!(e.layout.chain.is_empty());
        // the tiling changed (a 4x1 cannot keep a 2x2's list)
        let e = parse("matrix 32 32 4 1 tr col 1 0", &stored, 4096, &lim).unwrap();
        assert!(e.layout.chain.is_empty());
        // …unless the body brings a matching list along
        let e = parse("matrix 32 32 4 1 tl row 0 0\nchain 3,0,0 2,0,0 1,0,0 0,0,0", &stored, 4096, &lim).unwrap();
        assert_eq!(e.layout.chain.len(), 4);
    }

    /// On a panel board the arrangement is a table the driver swaps live
    /// (Gitea #920); only the framebuffer's shape needs a boot. A strip-built
    /// matrix keeps the #475 rule.
    #[test]
    fn the_arrangement_is_live_on_a_panel_board() {
        let mut cur = strip_layout();
        cur.kind = LayoutKind::Matrix;
        cur.matrix = Matrix::single(32, 32);
        cur.matrix.cols = 2;
        cur.matrix.rows = 2;
        let live = |body: &str| !parse(body, &cur, 4096, &panel_limits()).unwrap().reboot_required;
        assert!(live("matrix 32 32 2 2 tr col 1 90/270"), "rule fields");
        assert!(live("chain 1,0,90 1,1,90 0,1,270 0,0,270"), "an explicit list");
        assert!(!live("matrix 32 32 4 1 tl row 0 0"), "the tiling is the framebuffer's shape");
        assert!(!live("matrix 16 16 2 2 tl row 0 0"), "so is the tile size");
        // a strip-built matrix: still a boot, as since #475
        let e = parse("matrix 32 32 2 2 tr col 1 0", &cur, 4096, &big_limits()).unwrap();
        assert!(e.reboot_required);
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
        l.driver = PanelDriver { planes: 6, clock_mhz: 20, chip: Chip::Dp3246, blank: 4, lsb: 0, ring_ms: 3 };
        let wire = l.to_wire(6144, &proto_name);
        assert_eq!(
            wire,
            "matrix 64 32 3 1 br col 1 0 16 \npanel 6 20 dp3246 4 0 3\n\
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
        assert!(s.contains("\"matrix\":{\"pw\":64,\"ph\":64,\"cols\":1,\"rows\":1,\"start\":\"tl\",\"dir\":\"row\",\"snake\":0,\"rot\":[0,0],\"scan\":0,\"tiles\":[[0,0,0]],\"explicit\":false}"));
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
        v.panel = Some(PanelView { est_hz: 57, drive: 1, driver_live: None, card: Card::Off });
        let mut s = String::new();
        l.push_json(&mut s, &v);
        assert!(s.contains("\"rot\":[0,0],\"scan\":0,\"tiles\":[[0,0,0],[1,0,0]],\"explicit\":false,\"est_hz\":57,\"drive\":1,\"card\":\"off\"}"), "{s}");
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
            PanelDriver { planes: 5, clock_mhz: 24, chip: Chip::Fm6126a, blank: 0, lsb: 0, ring_ms: 3 }
        );
        assert_eq!(e.layout.driver.clock_hz(), 24_000_000);
        assert_eq!(e.layout.driver.latch_clocks(), 1);
        // the stored record IS the wire, so it reads back byte-for-byte —
        // and the persisted line always spells the fifth field (#789)
        let wire = e.layout.to_wire(4096, &proto_name);
        assert!(wire.contains("\npanel 5 24 fm6126a 0 0 3"), "{wire}");
        let load = Limits { strict: false, ..panel_limits() };
        assert_eq!(parse(&wire, &panel_cur(), 4096, &load).unwrap().layout, e.layout);
        // only the DP3246 holds the latch longer than one clock
        let e = parse("panel 8 30 dp3246 8", &cur, 4096, &panel_limits()).unwrap();
        assert_eq!(e.layout.driver.latch_clocks(), 3);
        assert_eq!(e.layout.driver.clock_hz(), 30_000_000);
        // a strip Layout has no panel, so its stored form carries no line
        assert!(!strip_layout().to_wire(60, &proto_name).contains("panel"));
    }

    #[test]
    fn the_driver_defaults_when_absent_and_a_post_without_it_merges() {
        // a fresh Layout is exactly what every HUB75 image shipped before the
        // line existed
        assert_eq!(
            panel_cur().driver,
            PanelDriver { planes: 7, clock_mhz: 30, chip: Chip::ShiftReg, blank: 1, lsb: 0, ring_ms: 3 }
        );
        assert_eq!(panel_cur().driver.clock_hz(), 30_000_000);
        // …and a body that says nothing about the driver KEEPS the stored one
        let mut cur = panel_cur();
        cur.driver = PanelDriver { planes: 4, clock_mhz: 12, chip: Chip::Icn2038s, blank: 3, lsb: 0, ring_ms: 3 };
        let e = parse("matrix 64 64 1 1 tl row 0 0", &cur, 4096, &panel_limits()).unwrap();
        assert!(!e.driver_set, "the body said nothing about it");
        assert_eq!(e.layout.driver, cur.driver, "merge, not reset");
    }

    #[test]
    fn a_bad_panel_line_names_the_field_it_rejected() {
        let cur = panel_cur();
        let cases: [(&str, &str); 15] = [
            ("panel 3 30 shiftreg 1", "planes"),
            ("panel 9 30 shiftreg 1", "planes"),
            ("panel 7 1 shiftreg 1", "clock_mhz"),
            ("panel 7 41 shiftreg 1", "clock_mhz"),
            // the clock is a LIST, so a value inside the old 2..40 range but
            // not on it is refused too — 40 was Jeremy's, 16 and 25 are the
            // ones esp-hal can only reach through its fractional divider
            // (Gitea #771)
            ("panel 7 40 shiftreg 1", "clock_mhz"),
            ("panel 7 16 shiftreg 1", "clock_mhz"),
            ("panel 7 25 shiftreg 1", "clock_mhz"),
            ("panel 7 30 fm6124 1", "chip"),
            ("panel 7 30 shiftreg 9", "blank"),
            ("panel 7 30 shiftreg 1 70000", "lsb"),
            ("panel 7 30 shiftreg 1 0 0", "ring_ms"),
            ("panel 7 30 shiftreg 1 0 51", "ring_ms"),
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
        // the refusal names the whole list, so a client that never read
        // `driver.clocks` still shows the user their options
        let e = parse("panel 7 40 shiftreg 1", &cur, 4096, &panel_limits()).unwrap_err();
        assert_eq!(e.msg, "panel: clock_mhz must be one of 8|10|12|15|20|24|30");
        // both ends of every other range ARE legal, and so is every listed clock
        for body in ["panel 4 8 shiftreg 0", "panel 8 30 dp3246 8"] {
            assert!(parse(body, &cur, 4096, &panel_limits()).is_ok(), "body {body:?}");
        }
        for mhz in PanelDriver::CLOCKS {
            let body = alloc::format!("panel 7 {mhz} shiftreg 1");
            assert!(parse(&body, &cur, 4096, &panel_limits()).is_ok(), "body {body:?}");
        }
    }

    /// The offered clocks are the ones esp-hal can reach with an INTEGER
    /// divider off an S3 LCD_CAM source — `source / (2 * N)` for XTAL (40 MHz)
    /// or PLL_D2 (240 MHz), since the i8080 driver doubles the request (Gitea
    /// #771). Ascending, 30 at the top, 8 at the bottom.
    #[test]
    fn every_offered_clock_is_an_exact_integer_divide() {
        let list = PanelDriver::CLOCKS;
        assert!(list.windows(2).all(|w| w[0] < w[1]), "ascending: {list:?}");
        assert_eq!(*list.last().unwrap(), 30, "the FM6124 datasheet ceiling");
        assert_eq!(list[0], 8, "below this a 7-plane 64x64 rescan flickers");
        assert_eq!(PanelDriver::default().clock_mhz, 30);
        assert!(PanelDriver::clock_supported(PanelDriver::default().clock_mhz));
        for mhz in list {
            // source MHz == clock MHz * 2 (esp-hal doubles the request) * N
            let hit = [40u32, 240]
                .iter()
                .any(|src| (2..=256).any(|n: u32| *src == u32::from(mhz) * 2 * n));
            assert!(hit, "{mhz} MHz is not an exact integer divide of 40 or 240 MHz");
        }
        for bad in [16u8, 25, 40, 0, 1, 2, 3, 4, 5, 6] {
            assert!(!PanelDriver::clock_supported(bad), "{bad} MHz must not be offered");
        }
    }

    #[test]
    fn changing_the_driver_needs_a_reboot() {
        let cur = panel_cur();
        // three of the four are read once, when the DMA is set up
        for body in ["panel 6 30 shiftreg 1", "panel 7 20 shiftreg 1", "panel 7 30 fm6126a 1"] {
            assert!(parse(body, &cur, 4096, &panel_limits()).unwrap().reboot_required, "{body}");
        }
        // restating the stored driver rebuilds nothing, so a Settings page may
        // POST the whole block on every edit
        assert!(!parse("panel 7 30 shiftreg 1", &cur, 4096, &panel_limits())
            .unwrap()
            .reboot_required);
        // and a driver written down on a STRIP Layout rebuilds nothing either
        let e = parse("panel 4 8 dp3246 8", &strip_layout(), 60, &strip_limits()).unwrap();
        assert!(!e.reboot_required, "a strip has no panel to rebuild");
    }

    /// Latch blanking is the one panel field a frame can apply (Gitea #778):
    /// it is control bits in the framebuffer words, which the packer never
    /// writes, so a host re-formats its buffers in place.
    #[test]
    fn latch_blanking_alone_applies_live() {
        let cur = panel_cur();
        for body in ["panel 7 30 shiftreg 0", "panel 7 30 shiftreg 4", "panel 7 30 shiftreg 8"] {
            let e = parse(body, &cur, 4096, &panel_limits()).unwrap();
            assert!(!e.reboot_required, "{body} is control bits, not a rebuild");
            assert!(e.driver_set);
        }
        // …but it does not EXCUSE the other three: the same body changing one
        // of them still waits for a boot
        for body in ["panel 6 30 shiftreg 4", "panel 7 20 shiftreg 4", "panel 7 30 dp3246 4"] {
            assert!(parse(body, &cur, 4096, &panel_limits()).unwrap().reboot_required, "{body}");
        }
        // the predicate itself, which is what a firmware asks outside a POST
        let d = PanelDriver::default();
        assert!(!d.boot_differs(&PanelDriver { blank: 7, ..d }));
        assert!(d.boot_differs(&PanelDriver { planes: 6, ..d }));
        assert!(d.boot_differs(&PanelDriver { clock_mhz: 20, ..d }));
        assert!(d.boot_differs(&PanelDriver { chip: Chip::Dp3246, ..d }));
        // and the latch width a host derives from a `live` reading's chip
        assert_eq!(Chip::ShiftReg.latch_clocks(), 1);
        assert_eq!(Chip::Fm6126a.latch_clocks(), 1);
        assert_eq!(Chip::Icn2038s.latch_clocks(), 1);
        assert_eq!(Chip::Dp3246.latch_clocks(), 3);
    }

    /// The fifth `panel` field — the brighter ↔ faster trade (Gitea #460 /
    /// #789). Optional on the way in so every stored Layout written before it
    /// existed keeps the full BCM schedule, and a BOOT field because the DMA
    /// descriptor chain is built from it.
    #[test]
    fn the_lsb_field_is_optional_and_boot_required() {
        let cur = panel_cur();
        // a FOUR-field line — every `panel` line written before #789 — is
        // lsb 0, the full on-time, which is what those layouts always meant
        let e = parse("panel 7 30 shiftreg 1", &cur, 4096, &panel_limits()).unwrap();
        assert!(e.driver_set);
        assert_eq!(e.layout.driver.lsb, 0, "absent = full on-time");
        assert!(!e.reboot_required, "…and it restates the stored default");
        // …and the field is read when it IS there
        let e = parse("panel 7 30 shiftreg 1 30", &cur, 4096, &panel_limits()).unwrap();
        assert_eq!(
            e.layout.driver,
            PanelDriver { planes: 7, clock_mhz: 30, chip: Chip::ShiftReg, blank: 1, lsb: 30, ring_ms: 3 }
        );
        assert!(e.reboot_required, "the descriptor chain is built at boot");
        // the predicate a firmware asks outside a POST agrees
        let d = PanelDriver::default();
        assert!(d.boot_differs(&PanelDriver { lsb: 30, ..d }));
        assert!(d.boot_differs(&PanelDriver { lsb: 1, ..d }));
        assert!(!d.boot_differs(&PanelDriver { lsb: 0, ..d }));
        // both ends of the range are legal; past the u16 ceiling is not, and
        // neither is a non-number
        for body in ["panel 7 30 shiftreg 1 0", "panel 7 30 shiftreg 1 65535"] {
            assert!(parse(body, &cur, 4096, &panel_limits()).is_ok(), "body {body:?}");
        }
        for body in ["panel 7 30 shiftreg 1 70000", "panel 7 30 shiftreg 1 x"] {
            let e = parse(body, &cur, 4096, &panel_limits()).unwrap_err();
            assert_eq!(
                e.msg, "panel: lsb must be 0..65535 (0 = full on-time)",
                "body {body:?}"
            );
        }
        // and a configured lsb survives the persist/reload round trip
        let mut l = panel_cur();
        l.driver.lsb = 30;
        let wire = l.to_wire(4096, &proto_name);
        assert!(wire.contains("\npanel 7 30 shiftreg 1 30"), "{wire}");
        let load = Limits { strict: false, ..panel_limits() };
        assert_eq!(parse(&wire, &panel_cur(), 4096, &load).unwrap().layout, l);
    }

    #[test]
    fn the_ring_ms_field_is_optional_and_boot_required() {
        let cur = panel_cur();
        // a FIVE-field line — every `panel` line written before #857 — keeps
        // the default slack, 3 ms
        let e = parse("panel 7 30 shiftreg 1 0", &cur, 4096, &panel_limits()).unwrap();
        assert_eq!(e.layout.driver.ring_ms, 3, "absent = the default slack");
        assert!(!e.reboot_required, "…and it restates the stored default");
        // …and the field is read when it IS there
        let e = parse("panel 7 30 shiftreg 1 0 10", &cur, 4096, &panel_limits()).unwrap();
        assert_eq!(e.layout.driver.ring_ms, 10);
        assert!(e.reboot_required, "the ring is allocated at boot");
        let d = PanelDriver::default();
        assert!(d.boot_differs(&PanelDriver { ring_ms: 1, ..d }));
        assert!(!d.boot_differs(&PanelDriver { ring_ms: 3, ..d }));
        for body in ["panel 7 30 shiftreg 1 0 1", "panel 7 30 shiftreg 1 0 50"] {
            assert!(parse(body, &cur, 4096, &panel_limits()).is_ok(), "body {body:?}");
        }
        for body in ["panel 7 30 shiftreg 1 0 0", "panel 7 30 shiftreg 1 0 51", "panel 7 30 shiftreg 1 0 x"] {
            let e = parse(body, &cur, 4096, &panel_limits()).unwrap_err();
            assert_eq!(e.msg, "panel: ring_ms must be 1..50", "body {body:?}");
        }
        // a configured slack survives the persist/reload round trip
        let mut l = panel_cur();
        l.driver.ring_ms = 10;
        let wire = l.to_wire(4096, &proto_name);
        assert!(wire.contains("\npanel 7 30 shiftreg 1 0 10"), "{wire}");
        let load = Limits { strict: false, ..panel_limits() };
        assert_eq!(parse(&wire, &panel_cur(), 4096, &load).unwrap().layout, l);
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
        l.driver = PanelDriver { planes: 6, clock_mhz: 20, chip: Chip::Fm6126a, blank: 2, lsb: 0, ring_ms: 3 };
        let mut v = view(&l, 4096, "null");
        v.panel = Some(PanelView { est_hz: 57, drive: 1, driver_live: None, card: Card::Off });
        let mut s = String::new();
        l.push_json(&mut s, &v);
        assert!(
            s.contains(
                "\"driver\":{\"planes\":6,\"clock_mhz\":20,\"chip\":\"fm6126a\",\"blank\":2,\
                 \"lsb\":0,\"ring_ms\":3,\
                 \"chips\":[\"shiftreg\",\"fm6126a\",\"icn2038s\",\"dp3246\"],\
                 \"clocks\":[8,10,12,15,20,24,30],\"live\":null}"
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
                lsb: 61,
                w: 64,
                h: 64,
                scan: 32,
                fb_bytes: 28672,
                fallback: true,
                ring_ms: 3,
                ring_rows: 9,
                ring_slack_us: 2870,
            }),
            card: Card::Off,
        });
        let mut s = String::new();
        l.push_json(&mut s, &v);
        assert!(
            s.contains(
                "\"live\":{\"planes\":7,\"clock_mhz\":30,\"chip\":\"shiftreg\",\"blank\":1,\
                 \"lsb\":61,\"ring_ms\":3,\
                 \"w\":64,\"h\":64,\"scan\":32,\"fb_bytes\":28672,\
                 \"ring_rows\":9,\"ring_slack_us\":2870,\"fallback\":true}}"
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
        v.map_grid = Some(GridMap::new(16, 8, false));
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

    // ---- the boot self-heal (Gitea #822) ----

    /// No `reverted` key on an ordinary body — the field is a self-heal
    /// report, and a body without one must stay byte-identical to what
    /// pre-#822 firmware wrote.
    #[test]
    fn no_revert_no_key() {
        let l = strip_layout();
        let mut s = String::new();
        l.push_json(&mut s, &view(&l, 300, "{}"));
        assert!(!s.contains("reverted"), "{s}");
    }

    #[test]
    fn revert_json_shape() {
        let l = strip_layout();
        let mut v = view(&l, 300, "{}");
        v.reverted = Some(Reverted { from_pixels: 8192, heap_free: 15_920 });
        let mut s = String::new();
        l.push_json(&mut s, &v);
        assert!(s.contains("\"reverted\":{\"from_pixels\":8192,\"heap_free\":15920}"), "{s}");
        // it sits between the driver block and the outputs, so a client
        // reading the body in order never has to look ahead
        let (r, o) = (s.find("\"reverted\"").unwrap(), s.find("\"outputs\"").unwrap());
        assert!(r < o, "{s}");
    }

    #[test]
    fn a_healthy_boot_reverts_nothing() {
        assert_eq!(heal_decision(64 * 1024, 20 * 1024, 8192, 4096, false, 0), Heal::Healthy);
        // exactly at the floor is healthy: the floor is what the firmware
        // needs to KEEP running, and it has it
        assert_eq!(heal_decision(20 * 1024, 20 * 1024, 8192, 4096, false, 0), Heal::Healthy);
    }

    #[test]
    fn a_starved_boot_reverts_a_bigger_stored_shape() {
        assert_eq!(heal_decision(15_920, 20 * 1024, 8192, 4096, false, 0), Heal::Revert);
    }

    /// The three ways the decision must refuse to reboot. Each of them is a
    /// boot loop if it gets this wrong.
    #[test]
    fn the_self_heal_never_loops() {
        // nothing smaller exists
        assert_eq!(heal_decision(15_920, 20 * 1024, 4096, 4096, true, 0), Heal::AtDefault);
        // the default is no smaller (a board whose default IS the big shape)
        assert_eq!(heal_decision(15_920, 20 * 1024, 4096, 8192, false, 0), Heal::NoSmaller);
        assert_eq!(heal_decision(15_920, 20 * 1024, 4096, 4096, false, 0), Heal::NoSmaller);
        // the revert already happened once and did not take (a refused flash
        // write); reverting again reboots forever
        assert_eq!(heal_decision(15_920, 20 * 1024, 8192, 4096, false, 8192), Heal::AlreadyReverted);
        // a DIFFERENT shape that has since been stored is a new case
        assert_eq!(heal_decision(15_920, 20 * 1024, 16_384, 4096, false, 8192), Heal::Revert);
    }

    /// `at_default` outranks everything: a board sitting at its own default
    /// with no heap left is a board too small for what is running on it, and
    /// the guard must say so rather than churn.
    #[test]
    fn at_default_outranks_the_pixel_comparison() {
        assert_eq!(heal_decision(0, 20 * 1024, 8192, 4096, true, 0), Heal::AtDefault);
    }
}
