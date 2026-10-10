//! What a device IS and what it CAN DO — the two blocks `GET /api/status`
//! carries as `geom` and `caps` (Gitea #464).
//!
//! Both are derived, never stored: `geom` from the running engine's
//! *effective* geometry (which is not the device map — a pattern that only
//! exports `render2D` runs on a fabricated ceil(√n) grid the device map knows
//! nothing about), `caps` from the host's fixed hardware facts plus that
//! geometry. The UI shows a setting only when its capability is advertised,
//! so the derivation lives HERE rather than in either host: the firmware and
//! the `luxel serve` mirror must answer the same question the same way, and
//! the rules are worth a host-side unit test.
//!
//! Everything in this module is pure and allocation-free apart from the two
//! `push_json` writers, which append to a caller-owned `String` through
//! `jsonview`'s push helpers (no `format!` — see `.claude/rules/firmware.md`).


use crate::jsonview::{push_piece, push_u32, Sink};
use crate::outpipe::GridMap;

/// Where the device's *own* map (not the engine's fallback) came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeviceMap {
    /// No device map at all — a bare strip board with nothing installed.
    None,
    /// The board's own geometry (a HUB75 panel IS a `PANEL_COLS`×`PANEL_ROWS`
    /// grid), installed because nothing was stored.
    Board,
    /// A map the user installed (`POST /api/map`, persisted in flash).
    User,
}

/// What produced the geometry the engine is actually rendering through.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GeomSource {
    /// A user-installed device map.
    User,
    /// The board's own geometry — a panel's grid, or a strip's bare 1D index
    /// space.
    Board,
    /// The engine's fabricated ceil(√n) square grid, installed because the
    /// pattern renders only in 2D/3D and nothing else supplied coordinates.
    /// Invisible in `/api/map`, which is why it needs saying here.
    Default,
}

impl GeomSource {
    pub fn name(self) -> &'static str {
        match self {
            GeomSource::User => "user",
            GeomSource::Board => "board",
            GeomSource::Default => "default",
        }
    }
}

/// The engine's EFFECTIVE geometry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Geom {
    /// 1, 2 or 3. `1` is a bare strip (index space, no map).
    pub dims: u8,
    /// True when the layout is a lattice the firmware can address as `w`×`h`
    /// — a strip (1×N) or a grid, procedural or recovered from a coordinate
    /// map by [`crate::outpipe::detect_grid`]. False for an irregular
    /// coordinate cloud, where `w`/`h` are 0 and the spatial output stages
    /// fall back to index space.
    pub regular: bool,
    /// Pixels along the fast axis; 0 when `regular` is false.
    pub w: u16,
    /// Rows along the slow axis; 1 on a strip, 0 when `regular` is false.
    pub h: u16,
    pub source: GeomSource,
    /// What the RUNNING PATTERN wants: 0 (no preference — `renderBulk` in
    /// index space), 1 (`render`), 2 (`render2D`), 3 (`render3D`). Differs
    /// from `dims` exactly when the pattern is being projected.
    pub pattern_dims: u8,
    /// False when the LAYOUT cannot show a pattern of this dimensionality at
    /// all (Gitea #538): a strip handed a 2D/3D pattern, a plane handed a 3D
    /// one. Note this is the layout's own dimensionality, not `dims` — a
    /// `source:"default"` geometry IS the engine papering over a strip, and
    /// is exactly the case a UI must flag.
    ///
    /// The frame still renders (the engine falls back to mid-space
    /// coordinates, and to its ceil(√n) grid for a 2D-only pattern) so a
    /// playlist entry, a share link or a Home Assistant call cannot black
    /// out a device. Hosts use this to hide or mark the pattern instead.
    pub compatible: bool,
}

impl Geom {
    /// The geometry of a strip of `pixel_count` pixels with nothing installed
    /// and no pattern loaded — what a host reports before its first engine.
    pub const fn strip(pixel_count: u32) -> Geom {
        Geom {
            dims: 1,
            regular: true,
            w: clamp_u16(pixel_count),
            h: 1,
            source: GeomSource::Board,
            pattern_dims: 0,
            compatible: true,
        }
    }

    /// Derive from what every host already knows about its running engine.
    ///
    /// - `dev_map` — [`DeviceMap`], the *device's* map and where it came from.
    /// - `engine_dims` — `Engine::installed_map().dims`, 0 when the engine has
    ///   no map at all. This is the one that sees the fabricated grid.
    /// - `grid` — `Engine::grid()`, `Some` only for a regular lattice.
    /// - `pixel_count` — the configured count (the strip case's `w`).
    /// - `pattern_dims` — `Engine::pattern_dims()`.
    pub fn derive(
        dev_map: DeviceMap,
        engine_dims: u8,
        grid: Option<GridMap>,
        pixel_count: u32,
        pattern_dims: u8,
    ) -> Geom {
        let dims = if engine_dims >= 2 { engine_dims.min(3) } else { 1 };
        let (regular, w, h) = match grid {
            Some(g) => (true, g.w, g.h),
            None if dims == 1 => (true, clamp_u16(pixel_count), 1),
            None => (false, 0, 0),
        };
        let source = match dev_map {
            DeviceMap::User => GeomSource::User,
            DeviceMap::Board => GeomSource::Board,
            // Nothing installed: a 2D/3D geometry can only be the engine's
            // own fallback, and a 1D one is just the board's index space.
            DeviceMap::None if dims >= 2 => GeomSource::Default,
            DeviceMap::None => GeomSource::Board,
        };
        // The LAYOUT's dimensionality, which `dims` is not: with no device
        // map the rig is a strip whatever the engine fabricated on top of it.
        let layout_dims = match dev_map {
            DeviceMap::None => 1,
            _ => dims,
        };
        let compatible = crate::projection::compatible(pattern_dims, layout_dims);
        Geom { dims, regular, w, h, source, pattern_dims, compatible }
    }

    /// `{"dims":D,"regular":B,"w":W,"h":H,"source":"…","pattern_dims":P,
    /// "compatible":B}`.
    pub fn push_json(&self, out: &mut dyn Sink) {
        push_piece(out, "{\"dims\":");
        push_u32(out, self.dims as u32);
        push_piece(out, ",\"regular\":");
        push_piece(out, bool_str(self.regular));
        push_piece(out, ",\"w\":");
        push_u32(out, self.w as u32);
        push_piece(out, ",\"h\":");
        push_u32(out, self.h as u32);
        push_piece(out, ",\"source\":\"");
        push_piece(out, self.source.name());
        push_piece(out, "\",\"pattern_dims\":");
        push_u32(out, self.pattern_dims as u32);
        push_piece(out, ",\"compatible\":");
        push_piece(out, bool_str(self.compatible));
        push_piece(out, "}");
    }
}

/// The fixed hardware facts a host knows about itself, before geometry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Hw {
    /// This host drives addressable LED strips (LED type, colour order and
    /// data pin are real settings on it).
    pub strip_driver: bool,
    /// This host drives a HUB75 matrix panel (clock/planes/scan are real).
    pub panel: bool,
    /// Physical LED outputs the BOARD has, whether or not the firmware drives
    /// them all yet (Gitea #474 makes the second one real).
    pub outputs: u8,
    /// Can reboot itself (setup AP, data-pin change, WiFi change).
    pub reboot: bool,
    /// Accepts a firmware image over the network.
    pub ota: bool,
    /// Has a second, external allocator for pattern arrays (Gitea #253).
    pub psram: bool,
    /// The device output chain's blur+glow stages fit this board's compose
    /// window. False on a board where they overrun it (Gitea #476/#446) —
    /// the pattern-side `setBlur`/`setGlow` are unaffected either way.
    pub blur_glow: bool,
}

/// What the UI may offer, per screen (proposal §5.3/§5.7). A field that is
/// false means the control is ABSENT, not disabled.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Caps {
    pub strip_driver: bool,
    pub panel: bool,
    pub outputs: u8,
    pub power_cap: bool,
    pub blur_glow: bool,
    pub layers: u8,
    pub text_slots: u8,
    pub reboot: bool,
    pub ota: bool,
    pub psram: bool,
    pub assets: bool,
    /// The device has a first-class SPRITE store (`/api/sprites`, Gitea
    /// #740). A console without it hides the Sprites tab and the scene
    /// editor's sprite layer.
    pub sprites: bool,
}

/// Text slots for host-supplied text (proposal §6, Gitea #485): how many
/// slots `GET/POST /api/text` addresses and `textSlot(n)` reads. It is
/// [`crate::text::SLOTS`] itself — the table lives in one place, so a
/// firmware and the mirror cannot advertise different sizes. A UI hides the
/// Scene text layer's "Text slot" source when this is 0.
pub const TEXT_SLOTS: u8 = crate::text::SLOTS as u8;

/// User-uploadable assets (fonts, images) as a scene-layer source. Not
/// planned (proposal §5.7): the whole-bundle assets partition is not a
/// per-file upload path, so every host advertises `false`.
pub const ASSETS: bool = false;

/// A first-class sprite store and its routes (Gitea #740). Every host that
/// carries this module has one — the record reader is `crate::sprite` and
/// the store is a few hundred bytes of route — so it is a constant, not an
/// `Hw` fact.
pub const SPRITES: bool = true;

/// Hard ceiling on advertised layers, whatever the arithmetic says — the
/// scene editor's own budget check (against live heap) is the real gate.
pub const MAX_LAYERS: u8 = 4;

/// Pattern layers a board of this pixel count affords
/// (`docs/design/webui-v2/research/engine-constraints.md` §2).
///
/// A layer costs a resident engine plus its own 3 B/px frame, and the binding
/// constraint at a large pixel count is that frame in internal DRAM, not the
/// arrays. The measured tiers: ~85 KB of headroom at ≤300 px on a classic
/// ESP32 (N=3 comfortably), ~80 KB at 1024 px with a 3 KB frame per layer
/// (N=2 safe, N=3 tight), ~36 KB on the S3 panel at 4096 px with a 12 KB
/// frame per layer (N=2 only) — which is the "2 on the S3 panel" the design
/// states. So the rule is a step at the point where the per-layer frame stops
/// being negligible, deliberately conservative: this number sizes the UI's
/// "2 of 2 used" note, and the scene editor still refuses an over-budget
/// layer with a reason.
///
/// The whole rationale is that INTERNAL frame, so the tier applies only
/// where the frame is internal. On a board whose frames live in a PSRAM
/// arena ([`layers_for_headroom`]'s `frame_external`) a layer costs its
/// program alone and the tier is [`MAX_LAYERS`]: the live arithmetic and
/// the ceiling decide (2026-09-30, the #709 follow-on). The `luxel serve`
/// mirror has no arena and keeps calling this directly.
pub const fn layers_for(pixel_count: u32) -> u8 {
    if pixel_count <= 512 {
        3
    } else {
        2
    }
}

/// [`layers_for`] narrowed by the heap the device actually has (Gitea #479).
///
/// The pixel-count tier above is a static *upper* bound on a board whose
/// layer frames are internal DRAM — it says what the board's shape affords
/// when the heap is healthy. This is the same number with two live
/// corrections applied (and, with `frame_external`, without the tier at all:
/// the tier's rationale is the internal frame, and there is none):
///
/// * `headroom` — what the device could spend on resident engines with the
///   current stack torn down, i.e.
///   `budget::load_headroom(budget::load_base(heap_free, engine_heap))`.
///   Measuring it that way rather than against bare `heap_free` keeps the
///   advertised number from dropping every time a scene loads. Each layer
///   costs [`crate::budget::layer_cost`]; a device under real memory
///   pressure advertises fewer layers instead of accepting a scene it will
///   then refuse at activation.
/// * `ceiling` — a hard per-board cap, [`MAX_LAYERS`] at most. The firmware
///   passes 2 on a `small-chip` board (the C3/C6 class, whose whole heap is
///   the size of one panel frame) and [`MAX_LAYERS`] elsewhere.
/// * `frame_external` — [`crate::arena::frames_external`], passed through to
///   [`crate::budget::layer_cost`]: on a `psram-arena` board the layer
///   engines' frames are not internal DRAM and a layer costs 4 KB rather
///   than 16 KB at 4096 px (Gitea #709). It also lifts the static tier to
///   [`MAX_LAYERS`], so the arithmetic and `ceiling` alone decide.
///
/// Never 0: a device that cannot afford a second layer still runs one, which
/// is the single-pattern case every board has always handled.
///
/// Worked numbers (docs/boards.md "Scene layers"): the S3 panel at 4096 px
/// with internal frames has ~36 KB of headroom against an 18 KB layer → 2,
/// and the tier says 2 → **2**; the same panel with its frames in the PSRAM
/// arena at 16,384 px (4x1) has ~28 KB against a 4 KB layer → 7, clamped by
/// [`MAX_LAYERS`] → **4**. A 300-px strip on a classic ESP32 has ~85 KB against a 7 KB layer
/// → 12, clamped by the tier → **3**. A c3-devkit is capped by `ceiling` →
/// **2**. A panel whose heap has been eaten by the device blur+glow chain
/// falls to **1** rather than promising a layer it cannot build.
pub const fn layers_for_headroom(
    pixel_count: u32,
    headroom: usize,
    ceiling: u8,
    frame_external: bool,
) -> u8 {
    let per = crate::budget::layer_cost(pixel_count, frame_external);
    // The compositor's own frame comes off the top FIRST (Gitea #704): a
    // scene composites into the host's staging buffer, 3 B/px, and that is
    // spent before a single layer engine is built. Leaving it out is what
    // made the Seengreat panel advertise 2 layers at 4096 px and then refuse
    // the second one at activation — 12.3 KB of the 32.8 KB two layers need
    // was already gone. An advertised capability has to be one the device
    // can actually deliver.
    let headroom =
        headroom.saturating_sub(crate::budget::compositor_scratch(pixel_count, frame_external));
    // Floored, and the host must pass a STEADY-STATE headroom (the
    // firmware's smoothed `shared::HEAP_BASE_EWMA`, then [`advertise`]'s
    // hysteresis on top) rather than an instantaneous one:
    // measuring costs heap, and rounding up over-promises. Both were tried
    // on the Seengreat panel 2026-09-24 — 26 KB of steady headroom against a
    // real ~15.3 KB per-layer cost at 4096 px, where one layer fits and two
    // do not, and rounding to nearest said two.
    // The `>` guard before the cast: a host with a huge heap would otherwise
    // TRUNCATE to a small u32 and read as starved.
    let n = headroom / per;
    let afford = if n > MAX_LAYERS as usize { MAX_LAYERS as u32 } else { n as u32 };
    // The tier is an internal-frame rule; with the frames external it does
    // not apply and MAX_LAYERS (via `cap` below) is the only static bound.
    let tier = if frame_external { MAX_LAYERS } else { layers_for(pixel_count) } as u32;
    let cap = if ceiling < MAX_LAYERS { ceiling } else { MAX_LAYERS } as u32;
    let n = if afford < tier { afford } else { tier };
    let n = if n < cap { n } else { cap };
    if n < 1 {
        1
    } else {
        n as u8
    }
}

/// Margin a smoothed `load_base` must clear past a layer boundary before
/// [`advertise`] moves the advertised count across it — in either direction.
///
/// Half a PSRAM-arena layer ([`crate::budget::LAYER_BASE`] / 2), so the
/// dead band around each boundary is a full layer wide on the board class
/// whose boundaries are closest together. It sits comfortably above the
/// residual ripple the [`fold_base`] average leaves: an alternating ±18 KB
/// reading (the panel's measured WiFi/HTTP transient, 2026-09-24) settles to
/// ±1.2 KB after the 1/8 fold.
pub const ADVERTISE_MARGIN: usize = 2 * 1024;

/// Fold one live `budget::load_base` reading into the running average the
/// firmware advertises `caps.layers` from (`shared::HEAP_BASE_EWMA`):
/// 7/8 of the old value plus 1/8 of the new. `prev` 0 means "no sample yet"
/// and seeds the average with the reading itself.
///
/// Pure integer arithmetic that cannot overflow (`prev − prev/8`, never
/// `prev × 7`), so a host with a very large heap folds just as well.
pub const fn fold_base(prev: usize, sample: usize) -> usize {
    if prev == 0 {
        sample
    } else {
        prev - prev / 8 + sample / 8
    }
}

/// The layer count to ADVERTISE, given the one advertised last time (`prev`,
/// 0 before the first call) and the smoothed `load_base` ([`fold_base`]) —
/// [`layers_for_headroom`] with hysteresis, so a board whose steady budget
/// sits right on a layer boundary does not flap between two answers.
///
/// The count moves only when the arithmetic says so with
/// [`ADVERTISE_MARGIN`] to spare: UP to what the base affords with the
/// margin taken OFF it, DOWN to what it affords with the margin ADDED. Both
/// are [`layers_for_headroom`] and it is monotonic in the headroom, so
/// `up ≤ down` and the two rules never pull against each other; between
/// them the previous answer holds. The first call (`prev` 0) returns the
/// plain arithmetic, and a change of pixel count or ceiling that lowers the
/// cap below `prev` moves down at once (the `down` answer is already capped).
pub const fn advertise(
    prev: u8,
    ewma_base: usize,
    pixel_count: u32,
    ceiling: u8,
    frame_external: bool,
) -> u8 {
    use crate::budget::load_headroom;
    if prev == 0 {
        return layers_for_headroom(
            pixel_count,
            load_headroom(ewma_base),
            ceiling,
            frame_external,
        );
    }
    let up = layers_for_headroom(
        pixel_count,
        load_headroom(ewma_base.saturating_sub(ADVERTISE_MARGIN)),
        ceiling,
        frame_external,
    );
    let down = layers_for_headroom(
        pixel_count,
        load_headroom(ewma_base.saturating_add(ADVERTISE_MARGIN)),
        ceiling,
        frame_external,
    );
    if up > prev {
        up
    } else if down < prev {
        down
    } else {
        prev
    }
}

impl Caps {
    /// Combine fixed hardware facts with the CURRENT layout.
    pub fn derive(hw: Hw, geom: &Geom, pixel_count: u32) -> Caps {
        Caps {
            strip_driver: hw.strip_driver,
            panel: hw.panel,
            outputs: hw.outputs,
            // A power cap needs a per-pixel current model. Strips have one; a
            // HUB75 panel is a fixed load on a supply sized for it, and the
            // firmware's scan-aware panel model is not something an installer
            // should be tuning (proposal §5.3).
            power_cap: !hw.panel,
            // Blur and glow need neighbours: the pixel index on a strip, rows
            // and columns on a regular grid. An irregular coordinate cloud has
            // no neighbour table, so the stages are hidden there — and a board
            // whose compose window they overrun hides them too (#476).
            blur_glow: hw.blur_glow && geom.regular,
            layers: layers_for(pixel_count),
            text_slots: TEXT_SLOTS,
            reboot: hw.reboot,
            ota: hw.ota,
            psram: hw.psram,
            assets: ASSETS,
            sprites: SPRITES,
        }
    }

    pub fn push_json(&self, out: &mut dyn Sink) {
        push_piece(out, "{\"strip_driver\":");
        push_piece(out, bool_str(self.strip_driver));
        push_piece(out, ",\"panel\":");
        push_piece(out, bool_str(self.panel));
        push_piece(out, ",\"outputs\":");
        push_u32(out, self.outputs as u32);
        push_piece(out, ",\"power_cap\":");
        push_piece(out, bool_str(self.power_cap));
        push_piece(out, ",\"blur_glow\":");
        push_piece(out, bool_str(self.blur_glow));
        push_piece(out, ",\"layers\":");
        push_u32(out, self.layers as u32);
        push_piece(out, ",\"text_slots\":");
        push_u32(out, self.text_slots as u32);
        push_piece(out, ",\"reboot\":");
        push_piece(out, bool_str(self.reboot));
        push_piece(out, ",\"ota\":");
        push_piece(out, bool_str(self.ota));
        push_piece(out, ",\"psram\":");
        push_piece(out, bool_str(self.psram));
        push_piece(out, ",\"assets\":");
        push_piece(out, bool_str(self.assets));
        push_piece(out, ",\"sprites\":");
        push_piece(out, bool_str(self.sprites));
        push_piece(out, "}");
    }
}

const fn bool_str(b: bool) -> &'static str {
    if b {
        "true"
    } else {
        "false"
    }
}

const fn clamp_u16(v: u32) -> u16 {
    if v > u16::MAX as u32 {
        u16::MAX
    } else {
        v as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(w: u16, h: u16) -> Option<GridMap> {
        Some(GridMap::new(w, h, false))
    }

    #[test]
    fn bare_strip_is_one_dimensional_and_regular() {
        let g = Geom::derive(DeviceMap::None, 0, None, 60, 1);
        assert_eq!(g.dims, 1);
        assert!(g.regular);
        assert_eq!((g.w, g.h), (60, 1));
        assert_eq!(g.source, GeomSource::Board);
        assert_eq!(g.pattern_dims, 1);
    }

    #[test]
    fn fabricated_square_grid_reads_as_default() {
        // a render2D-only pattern on a 60 px strip with no device map:
        // Engine::set_default_grid_map() gives ceil(sqrt(60)) = 8 wide
        let g = Geom::derive(DeviceMap::None, 2, grid(8, 8), 60, 2);
        assert_eq!(g.dims, 2);
        assert!(g.regular);
        assert_eq!((g.w, g.h), (8, 8));
        assert_eq!(g.source, GeomSource::Default, "the √n grid must be visible");
    }

    #[test]
    fn panel_board_grid_reads_as_board() {
        let g = Geom::derive(DeviceMap::Board, 2, grid(64, 64), 4096, 2);
        assert_eq!((g.dims, g.w, g.h), (2, 64, 64));
        assert_eq!(g.source, GeomSource::Board);
    }

    #[test]
    fn user_coord_map_that_detects_as_a_grid_is_regular() {
        let g = Geom::derive(DeviceMap::User, 2, grid(16, 8), 128, 2);
        assert!(g.regular);
        assert_eq!((g.w, g.h), (16, 8));
        assert_eq!(g.source, GeomSource::User);
    }

    #[test]
    fn irregular_user_map_has_no_shape() {
        let g = Geom::derive(DeviceMap::User, 3, None, 300, 3);
        assert_eq!(g.dims, 3);
        assert!(!g.regular);
        assert_eq!((g.w, g.h), (0, 0));
        assert_eq!(g.source, GeomSource::User);
    }

    #[test]
    fn strip_json_shape() {
        let mut s = String::new();
        Geom::strip(60).push_json(&mut s);
        assert_eq!(
            s,
            "{\"dims\":1,\"regular\":true,\"w\":60,\"h\":1,\"source\":\"board\",\
             \"pattern_dims\":0,\"compatible\":true}"
        );
    }

    /// Gitea #538: a Layout shows its own dimensionality and lower. The
    /// interesting case is the fabricated grid — `dims` says 2, but the rig
    /// underneath is a bare strip, so the pattern is NOT compatible.
    #[test]
    fn compatible_reads_the_layout_not_the_fabricated_grid() {
        let fabricated = Geom::derive(DeviceMap::None, 2, Some(GridMap::new(8, 8, false)), 64, 2);
        assert_eq!(fabricated.source, GeomSource::Default);
        assert_eq!(fabricated.dims, 2);
        assert!(!fabricated.compatible);
        // a strip running a strip pattern, and one with no engine yet
        assert!(Geom::derive(DeviceMap::None, 0, None, 60, 1).compatible);
        assert!(Geom::derive(DeviceMap::None, 0, None, 60, 0).compatible);
        // a real panel: 1D and 2D patterns yes, 3D no
        let panel = |pd| Geom::derive(DeviceMap::Board, 2, Some(GridMap::new(64, 64, false)), 4096, pd);
        assert!(panel(1).compatible);
        assert!(panel(2).compatible);
        assert!(!panel(3).compatible);
        // a user 3D map takes everything
        for pd in 0..=3 {
            assert!(Geom::derive(DeviceMap::User, 3, None, 300, pd).compatible, "{pd}");
        }
    }

    const STRIP_HW: Hw = Hw {
        strip_driver: true,
        panel: false,
        outputs: 2,
        reboot: true,
        ota: true,
        psram: false,
        blur_glow: true,
    };
    const PANEL_HW: Hw = Hw {
        strip_driver: false,
        panel: true,
        outputs: 1,
        reboot: true,
        ota: true,
        psram: true,
        blur_glow: false,
    };

    #[test]
    fn strip_board_caps() {
        let g = Geom::derive(DeviceMap::None, 0, None, 60, 1);
        let c = Caps::derive(STRIP_HW, &g, 60);
        assert!(c.strip_driver && !c.panel);
        assert_eq!(c.outputs, 2);
        assert!(c.power_cap, "strips have a per-pixel current model");
        assert!(c.blur_glow, "index-space neighbours exist on a strip");
        assert_eq!(c.layers, 3);
        assert_eq!(c.text_slots, 8);
        assert!(!c.assets);
        assert!(c.sprites, "every host with luxel-core has a sprite store");
    }

    #[test]
    fn panel_board_caps() {
        let g = Geom::derive(DeviceMap::Board, 2, grid(64, 64), 4096, 2);
        let c = Caps::derive(PANEL_HW, &g, 4096);
        assert!(!c.strip_driver && c.panel);
        assert!(!c.power_cap, "a panel is a fixed load, not a per-pixel one");
        assert!(!c.blur_glow, "the S3 panel's compose window can't take them");
        assert_eq!(c.layers, 2, "2 on the S3 panel");
        assert!(c.psram);
    }

    #[test]
    fn irregular_layout_hides_blur_glow() {
        let g = Geom::derive(DeviceMap::User, 3, None, 300, 3);
        let c = Caps::derive(STRIP_HW, &g, 300);
        assert!(!c.blur_glow, "no neighbour table off a regular lattice");
        assert!(c.power_cap, "the current model is per pixel, not per shape");
    }

    #[test]
    fn layer_tiers() {
        assert_eq!(layers_for(60), 3);
        assert_eq!(layers_for(512), 3);
        assert_eq!(layers_for(513), 2);
        assert_eq!(layers_for(4096), 2);
        assert!(layers_for(4096) <= MAX_LAYERS);
    }

    #[test]
    fn caps_json_shape() {
        let g = Geom::derive(DeviceMap::None, 0, None, 60, 0);
        let mut s = String::new();
        Caps::derive(STRIP_HW, &g, 60).push_json(&mut s);
        assert_eq!(
            s,
            "{\"strip_driver\":true,\"panel\":false,\"outputs\":2,\"power_cap\":true,\
             \"blur_glow\":true,\"layers\":3,\"text_slots\":8,\"reboot\":true,\"ota\":true,\
             \"psram\":false,\"assets\":false,\"sprites\":true}"
        );
    }
}

#[cfg(test)]
mod layer_tests {
    use super::*;
    use crate::budget::{load_base, load_headroom};

    fn layers(pixels: u32, heap_free: usize, engine_heap: usize, ceiling: u8) -> u8 {
        layers_for_headroom(
            pixels,
            load_headroom(load_base(heap_free, engine_heap)),
            ceiling,
            false,
        )
    }

    /// The same board with its engine frames in the PSRAM arena (#709).
    fn layers_psram(pixels: u32, heap_free: usize, engine_heap: usize, ceiling: u8) -> u8 {
        layers_for_headroom(
            pixels,
            load_headroom(load_base(heap_free, engine_heap)),
            ceiling,
            true,
        )
    }

    /// Gitea #479's acceptance numbers, board by board.
    #[test]
    fn the_measured_boards_land_where_the_design_says() {
        // Seengreat S3 @4096 px, rainbow resident (docs/boards.md:611)
        assert_eq!(layers(4096, 51_704, 17_000, MAX_LAYERS), 2);
        // …and the SAME board's STEADY-STATE numbers measured on metal
        // 2026-09-24, which are lower (the JIT image's statics plus Phase
        // B/C's): 26 KB of headroom affords one 4096-px layer, not two.
        assert_eq!(layers(4096, 31_200, 15_348, MAX_LAYERS), 1);
        // The panel's own boot-time high-water, which is what the firmware
        // fed this until 2026-09-30 (`HEAP_BASE_MAX`): before #704 it read
        // 2 from the layer arithmetic alone and then refused layer 2 at
        // activation, because the 12.3 KB staging frame every scene needs
        // had not been charged to anything. Measured `load_base` 49,120 on
        // metal 2026-09-24, and even a 56 KB high-water is one layer.
        assert_eq!(layers(4096, 49_120, 0, MAX_LAYERS), 1);
        assert_eq!(layers(4096, 56_000, 0, MAX_LAYERS), 1);
        // two layers at 4096 px need the floor, two engines AND the stage:
        // 20,480 + 2x16,384 + 12,288
        assert_eq!(layers(4096, 65_536, 0, MAX_LAYERS), 2);
        // Athom / classic ESP32 @300 px idle (104,832 B)
        assert_eq!(layers(300, 104_832, 18_000, MAX_LAYERS), 3);
        // classic ESP32 @1024 px — the tier caps it at 2
        assert_eq!(layers(1024, 90_000, 18_000, MAX_LAYERS), 2);
        // a small-chip board is capped whatever the arithmetic says
        assert_eq!(layers(300, 104_832, 18_000, 2), 2);
    }

    /// Gitea #709: the panel's frames move to the PSRAM arena and its
    /// MEASURED steady-state numbers — the ones that said 1 above — deliver
    /// at least the pair. With the compositor's scratch in the arena as
    /// well, the 20,480 floor leaves 26.6 KB of the 47.1 KB low reading,
    /// which is six 4,096 B layers — clamped by the tier to 2 until the #709
    /// follow-on (2026-09-30) dropped the tier for external frames, and by
    /// [`MAX_LAYERS`] to 4 since.
    #[test]
    fn psram_frames_make_the_panels_second_layer_real() {
        assert_eq!(layers_psram(4096, 47_121, 0, MAX_LAYERS), 4);
        // 46,548 − 20,480 = 26,068 → six layers → 4
        assert_eq!(layers_psram(4096, 31_200, 15_348, MAX_LAYERS), 4);
        // the panel's 2026-09-26 idle reading (35.8 KB after #770's rings
        // and tables moved onto the heap): 15,320 B → three layers
        assert_eq!(layers_psram(4096, 33_248, 2_552, MAX_LAYERS), 3);
        // it is not a blank cheque: a genuinely starved board still says 1
        assert_eq!(layers_psram(4096, 24_576, 0, MAX_LAYERS), 1);
        assert_eq!(layers_psram(4096, 0, 0, MAX_LAYERS), 1);
        // the exact two-layer edge: 20,480 floor + 2x4,096
        assert_eq!(layers_psram(4096, 28_672, 0, MAX_LAYERS), 2);
        assert_eq!(layers_psram(4096, 28_671, 0, MAX_LAYERS), 1);
    }

    /// The #709 follow-on (2026-09-30): with frames external there is no
    /// pixel-count tier, so an arena board advertises what its heap affords,
    /// up to [`MAX_LAYERS`] — at 4096 px AND at the 4x1 chain's 16,384 px,
    /// where a layer still costs only its 4 KB program.
    #[test]
    fn an_arena_board_is_bounded_by_its_heap_not_the_tier() {
        // the Seengreat 4x1 (16,384 px) as measured 2026-09-30: heap_free
        // 39.5 KB with two engines resident, engine_heap 8–12 KB →
        // load_base ~48–52 KB → 7 layers of headroom → MAX_LAYERS
        assert_eq!(layers_psram(16_384, 39_500, 10_000, MAX_LAYERS), 4);
        assert_eq!(layers_psram(16_384, 39_500, 8_000, MAX_LAYERS), 4);
        // …and the same numbers at 4096 px, which a pixel count never moved
        assert_eq!(layers_psram(4096, 39_500, 10_000, MAX_LAYERS), 4);
        // the exact edges: floor + 3 × 4,096 and floor + 4 × 4,096
        assert_eq!(layers_psram(16_384, 32_768, 0, MAX_LAYERS), 3);
        assert_eq!(layers_psram(16_384, 32_767, 0, MAX_LAYERS), 2);
        assert_eq!(layers_psram(16_384, 36_864, 0, MAX_LAYERS), 4);
        // a starved arena board still says 1, and the ceiling still binds
        assert_eq!(layers_psram(16_384, 24_000, 0, MAX_LAYERS), 1);
        assert_eq!(layers_psram(16_384, 39_500, 10_000, 2), 2);
        // the SAME 16,384 px with internal frames would be the tier's 2 at
        // most — and in practice 1, since one 48 KB frame does not fit
        assert_eq!(layers(16_384, 39_500, 10_000, MAX_LAYERS), 1);
    }

    #[test]
    fn a_starved_device_advertises_one_not_zero() {
        // the panel with the device blur+glow chain eating the heap
        assert_eq!(layers(4096, 26_928, 0, MAX_LAYERS), 1);
        assert_eq!(layers(4096, 0, 0, MAX_LAYERS), 1);
        assert_eq!(layers_for_headroom(4096, 0, 0, false), 1);
        assert_eq!(layers_for_headroom(4096, 0, 0, true), 1);
    }

    #[test]
    fn it_never_exceeds_the_static_tier_or_the_hard_ceiling() {
        // An absurdly roomy host is still bounded by the pixel-count tier,
        // which is what keeps MAX_LAYERS a backstop rather than a target.
        assert_eq!(
            layers_for_headroom(64, usize::MAX / 2, 100, false),
            layers_for(64)
        );
        assert_eq!(
            layers_for_headroom(4096, usize::MAX / 2, MAX_LAYERS, false),
            layers_for(4096)
        );
        assert!(layers_for_headroom(64, usize::MAX / 2, 100, false) <= MAX_LAYERS);
        // with frames external the tier is lifted: MAX_LAYERS binds instead
        assert_eq!(
            layers_for_headroom(4096, usize::MAX / 2, MAX_LAYERS, true),
            MAX_LAYERS
        );
        assert_eq!(
            layers_for_headroom(64, usize::MAX / 2, 100, true),
            MAX_LAYERS
        );
    }
}

/// The firmware's advertise loop (`server::scene_layer_cap`): every call
/// folds a live `load_base` into the average and re-derives the count with
/// hysteresis. These drive that loop over synthetic readings.
#[cfg(test)]
mod advertise_tests {
    use super::*;
    use crate::budget::{compositor_scratch, layer_cost, RUNTIME_FLOOR};

    /// One firmware call: fold, then advertise.
    fn step(state: &mut (usize, u8), sample: usize, px: u32, external: bool) -> u8 {
        state.0 = fold_base(state.0, sample);
        state.1 = advertise(state.1, state.0, px, MAX_LAYERS, external);
        state.1
    }

    #[test]
    fn the_seeded_first_call_is_the_arithmetic() {
        for &(px, base, ext) in &[
            (4096u32, 65_536usize, false),
            (4096, 65_535, false),
            (16_384, 48_000, true),
            (16_384, 24_000, true),
            (300, 104_832, false),
        ] {
            let mut st = (0usize, 0u8);
            assert_eq!(
                step(&mut st, base, px, ext),
                layers_for_headroom(px, crate::budget::load_headroom(base), MAX_LAYERS, ext)
            );
            assert_eq!(st.0, base, "the first fold seeds the average");
        }
    }

    /// The 2026-09-24 flap: identical activations read ±18 KB apart, against
    /// a 16 KB internal-frame layer, and the live number went 1 ↔ 2 with
    /// nothing but poll traffic. Straddle the 1→2 boundary at 4096 px
    /// exactly and the advertised count must settle and stay put.
    #[test]
    fn a_base_oscillating_across_a_boundary_does_not_flap() {
        let px = 4096;
        let edge = RUNTIME_FLOOR + compositor_scratch(px, false) + 2 * layer_cost(px, false);
        assert_eq!(edge, 65_536);
        for &(mean, first_high) in &[
            (edge, true),
            (edge, false),
            (edge - 1_000, true),
            (edge + 1_000, false),
        ] {
            let mut st = (0usize, 0u8);
            let mut seen = [0u8; 200];
            for (i, slot) in seen.iter_mut().enumerate() {
                let high = (i % 2 == 0) == first_high;
                let s = if high { mean + 18_000 } else { mean - 18_000 };
                *slot = step(&mut st, s, px, false);
            }
            let changes = seen.windows(2).filter(|w| w[0] != w[1]).count();
            assert!(changes <= 1, "flapped {} times around {}", changes, mean);
            // once the average has settled, not a single change
            assert!(seen[40..].windows(2).all(|w| w[0] == w[1]));
        }
        // the same on an arena board whose boundaries are 4 KB apart, with
        // a ±18 KB ripple around the 2→3 edge
        let px = 16_384;
        let edge = RUNTIME_FLOOR + 3 * layer_cost(px, true);
        let mut st = (0usize, 0u8);
        let mut seen = [0u8; 200];
        for (i, slot) in seen.iter_mut().enumerate() {
            let s = if i % 2 == 0 {
                edge + 18_000
            } else {
                edge - 18_000
            };
            *slot = step(&mut st, s, px, true);
        }
        assert!(seen[40..].windows(2).all(|w| w[0] == w[1]));
    }

    /// Real headroom that arrives and STAYS (a scene torn down, a transient
    /// gone for good) must show up — the average is a lag, not a ratchet.
    #[test]
    fn a_base_that_rises_and_stays_moves_up_within_a_few_folds() {
        let px = 16_384;
        let mut st = (0usize, 0u8);
        // 30,000 − 20,480 = 9,520 → two 4 KB layers
        for _ in 0..50 {
            step(&mut st, 30_000, px, true);
        }
        assert_eq!(st.1, 2);
        // +20 KB: 50,000 affords seven, i.e. MAX_LAYERS
        let mut folds = 0;
        while st.1 < MAX_LAYERS {
            step(&mut st, 50_000, px, true);
            folds += 1;
            assert!(folds <= 16, "still {} after {} folds", st.1, folds);
        }
        // and it moves back down when the headroom goes away for good
        let mut folds = 0;
        while st.1 > 2 {
            step(&mut st, 30_000, px, true);
            folds += 1;
            assert!(folds <= 32, "still {} after {} folds", st.1, folds);
        }
        assert_eq!(st.1, 2);
    }

    /// Inside the dead band the previous answer holds in both directions.
    #[test]
    fn the_margin_is_a_dead_band_around_each_boundary() {
        let px = 16_384;
        let edge = RUNTIME_FLOOR + 3 * layer_cost(px, true); // 2 → 3
        for d in [0usize, 1, ADVERTISE_MARGIN - 1] {
            assert_eq!(advertise(2, edge + d, px, MAX_LAYERS, true), 2);
            assert_eq!(advertise(3, edge + d, px, MAX_LAYERS, true), 3);
        }
        assert_eq!(
            advertise(2, edge + ADVERTISE_MARGIN, px, MAX_LAYERS, true),
            3
        );
        assert_eq!(advertise(3, edge - 1, px, MAX_LAYERS, true), 3);
        assert_eq!(
            advertise(3, edge - ADVERTISE_MARGIN, px, MAX_LAYERS, true),
            3
        );
        assert_eq!(
            advertise(3, edge - ADVERTISE_MARGIN - 1, px, MAX_LAYERS, true),
            2
        );
        // a cap that drops below `prev` (ceiling, pixel count) binds at once
        assert_eq!(advertise(4, 1_000_000, px, 2, true), 2);
        assert_eq!(advertise(3, 1_000_000, 4096, MAX_LAYERS, false), 2);
    }

    #[test]
    fn the_fold_is_a_seeded_eighth() {
        assert_eq!(fold_base(0, 40_000), 40_000);
        assert_eq!(fold_base(40_000, 48_000), 41_000);
        // never overflows, however large the heap
        assert_eq!(fold_base(usize::MAX, usize::MAX), usize::MAX);
    }
}
