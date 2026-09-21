//! Frame pipeline: compile a pattern, run its init code, then per frame run
//! `beforeRender(delta)` and the best-matching render function per pixel.
//!
//! Runtime errors never stop the engine — a pattern-level error aborts only
//! the current handler invocation (recorded as `vmerr`; the frame keeps
//! going, matching PB's blast radius — tools/oracle/oob-probes.mjs). Only
//! `assert()` failures and VM resource guards end the frame early.
//!
//! Pixel maps install via [`Engine::set_map`] (host-normalized to world
//! units 0..1 exclusive); render selection follows the documented priority
//! per map dimensionality, missing coordinates fill with mid-space (0.5),
//! and the transform stack applies to 2D/3D coordinates. Oracle-verified
//! 2026-07-07: composition order (first call outermost), cross-frame
//! accumulation, and rotate direction all match PB; 1D-x remains
//! unverifiable on our oracle: a PB that has ever saved a map can never be
//! made mapless again through its public API (see 04-oracle-findings.md).

use alloc::string::String;
use alloc::vec::Vec;

#[cfg(feature = "frontend")]
use crate::compile::compile;
#[cfg(feature = "frontend")]
use crate::diag::Diagnostic;
use crate::fixed::Fx;
use crate::projection::{dims as norm_dims, Projection, ProjectionMode};
use crate::vm::{ArrView, DebugState, MapData, Outcome, Program, StepKind, Value, Vm, VmError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RenderKind {
    R1(u16),
    R2(u16),
    R3(u16),
    /// `renderFrame()` — the whole-frame entry: one zero-argument call per
    /// frame instead of one call per pixel, with the frame buffer lent to
    /// the VM so the bulk builtins (`crate::bulk`) write it directly.
    Frame(u16),
}

/// A render entry candidate. PB also dispatches `render`/`render2D`/
/// `render3D` through a plain GLOBAL of that name when a pattern assigns
/// it a function at runtime (`export var render2D` + `render2D = fn` in
/// `beforeRender` — oracle-confirmed 2026-08-29, tools/oracle/
/// alias-probes.mjs, incl. live re-assignment between frames). A Global
/// candidate only wins selection while it currently holds a function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RenderTarget {
    Fn(u16),
    Global(u16),
}

/// Where a debug-paused frame pipeline is suspended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RunStage {
    Before,
    /// Inside `renderFrame`. The engine's `pixels` Vec lives in
    /// `Vm::frame` for the duration; [`Engine::frame_buffer_in`] always
    /// takes it back before this stage's outcome is inspected, including
    /// on a debug pause, so `pixels()` is never empty behind the caller's
    /// back.
    Frame,
    Pixel(u32),
}

/// One entry of the paused call stack, for debugger UIs.
#[derive(Clone, Debug)]
pub struct DebugFrame {
    pub name: String,
    pub fn_idx: u16,
    pub pc: u32,
    pub line: u32,
    pub col: u32,
    pub locals: Vec<(String, Value)>,
}

/// A UI control exported by the pattern (`export function sliderSpeed(v)`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Control {
    pub kind: ControlKind,
    /// Name with the prefix stripped (`sliderSpeed` → `Speed`).
    pub label: String,
    /// Full exported function name.
    pub name: String,
    fn_idx: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlKind {
    Slider,
    HsvPicker,
    RgbPicker,
    Toggle,
    Trigger,
    InputNumber,
    ShowNumber,
    Gauge,
}

const CONTROL_PREFIXES: &[(&str, ControlKind)] = &[
    ("slider", ControlKind::Slider),
    ("hsvPicker", ControlKind::HsvPicker),
    ("rgbPicker", ControlKind::RgbPicker),
    ("toggle", ControlKind::Toggle),
    ("trigger", ControlKind::Trigger),
    ("inputNumber", ControlKind::InputNumber),
    ("showNumber", ControlKind::ShowNumber),
    ("gauge", ControlKind::Gauge),
];

/// How a frame's render calls are mapped onto the Layout's pixels — the
/// executable form of one cell of the §5.4d projection table, recomputed
/// only when the pattern's entry, the Layout or the projection changes.
/// Every default resolves to [`ProjPlan::Native`], so an engine nobody has
/// configured runs exactly the code path it always did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProjPlan {
    /// The pattern is native to the Layout (or the projection is a no-op):
    /// one render call per pixel with the Layout's own coordinates.
    Native,
    /// Coordinate substitution: `sel[k]` says which Layout coordinate
    /// (`0`/`1`/`2`) feeds the pattern's k-th coordinate argument, or `3`
    /// for mid-space (0.5). One render call per pixel, as before.
    Select([u8; 3]),
    /// Render ONE strip of `len` pixels — `pixelCount` reads as `len`, the
    /// pattern sees no map — then replicate it across the Layout along
    /// `axis` (0 = x, 1 = y, 2 = z). This is the engine win: a 1D pattern on
    /// a 64×64 panel costs 64 render calls, not 4096.
    Strip { axis: u8, len: u32 },
    /// A `renderFrame` (whole-frame, 2D) pattern on a 1D Layout — an
    /// incompatible pairing no host offers (#538), but one the engine still
    /// has to render: it gets a w×1 grid so the grid-space bulk builtins
    /// describe the strip instead of a grid that is not there.
    FrameGrid(crate::outpipe::GridMap),
}

/// What the pattern actually sees this frame once the projection is applied
/// — the geometry a UI must caption, thumbnail and size its preview from
/// ([`Engine::effective_geometry`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectiveGeometry {
    /// What `pixelCount` reads inside the pattern (the strip length under an
    /// along-axis projection, the Layout's pixel count otherwise).
    pub pixel_count: u32,
    /// The space the pattern renders in: 0 = none (a dimensionless
    /// index-space `renderFrame` — native on every Layout), 1 = strip,
    /// 2 = plane, 3 = volume. The RESOLVED entry's dimensionality, so it can
    /// differ from `/api/status`'s `geom.pattern_dims` (what the pattern
    /// declares) for a pattern that exports more than one render entry.
    pub pattern_dims: u8,
    /// The Layout's own dimensionality.
    pub layout_dims: u8,
    /// Grid the pattern's grid-space builtins see, `(0, 0)` when there is none.
    pub w: u16,
    pub h: u16,
    /// The projection in force, or `None` when the pattern is native — or
    /// when it is incompatible, which has no projection to be in force.
    pub mode: Option<ProjectionMode>,
    /// False when this Layout cannot show a pattern of this dimensionality
    /// at all (#538): a strip handed a 2D/3D pattern, a plane handed a 3D
    /// one. The frame still renders — on the engine's plain fallback
    /// coordinates, see [`crate::projection::compatible`] — so a host that
    /// activates one anyway (an old playlist entry, a share link, HA) does
    /// not go dark; this is the flag a UI hides or marks the tile with.
    pub compatible: bool,
}

/// One frame of sensor-board data (the PB sensor expansion board surface).
/// Everything is normalized 0..1 except `accelerometer` (signed G-ish) and
/// `max_frequency` (Hz of the loudest bin).
#[derive(Clone, Default)]
pub struct SensorFrame {
    pub frequency_data: [Fx; 32],
    pub energy_average: Fx,
    pub max_frequency_magnitude: Fx,
    pub max_frequency: Fx,
    pub light: Fx,
    pub accelerometer: [Fx; 3],
    pub analog_inputs: [Fx; 5],
}

pub struct Engine {
    prog: Program,
    vm: Vm,
    pixel_count: u32,
    before: Option<u16>,
    /// Entry candidates for render/render2D/render3D/renderFrame (fixed
    /// at load).
    render_tgt: [Option<RenderTarget>; 4],
    /// The resolved entry for the current frame (see [`resolve_render`]).
    render: Option<RenderKind>,
    controls: Vec<Control>,
    pixels: Vec<[u8; 3]>,
    /// Pattern-local time in ms, 16-frac for sub-ms delta accumulation.
    time_acc: u64,
    /// `time_acc` as of the last frame the pattern actually ran — the
    /// difference is `beforeRender`'s delta, so a frame-rate-capped
    /// pattern sees the whole elapsed interval, not one host tick.
    render_time_acc: u64,
    /// Real (unscaled) ms since the last pattern render, 16-frac —
    /// `setFrameRate`'s accounting. Real time, not pattern time, so a
    /// `timeScale(0)` freeze doesn't also stop rendering.
    frame_acc: u64,
    /// False until the first frame runs, so a cap set during init can't
    /// hold a never-rendered (black) buffer.
    rendered_once: bool,
    pub last_error: Option<VmError>,
    /// The pattern's own INIT was refused an array (Gitea #420), so the
    /// render pass records nothing for the rest of this engine's life.
    ///
    /// `array(pixelCount)` at the top level is how buffer patterns declare
    /// their channels, and when one is refused every frame afterwards fails
    /// on the missing buffer — "indexing a non-array value", at a fresh site
    /// per handler. Hosts poll `take_error` once a frame and publish the
    /// newest message (`/api/status`'s vmerr, the playground's runtime
    /// banner), so the refusal that says WHY was visible for one frame and
    /// then buried under its own cascade for as long as the pattern ran.
    /// Silencing the cascade leaves the refusal standing as the last thing
    /// every host was told, which is the honest answer: nothing downstream
    /// of a missing buffer is diagnostic. It also stops a `format!` per
    /// erroring site per frame on a device that is already out of memory.
    arrays_refused: bool,
    debug_enabled: bool,
    /// Some(stage) while the pipeline is suspended at a debug stop.
    run_stage: Option<RunStage>,
    cur_delta: Fx,
    /// 256-entry output curve for `setGamma` — rebuilt only when the value
    /// changes, so the per-pixel cost is one table lookup, not a pow().
    gamma_lut: Option<alloc::boxed::Box<[u8; 256]>>,
    gamma_lut_for: Fx,
    /// 256-entry luma → color table for `setOutputPalette`, rebuilt only
    /// when the installed palette changes (tracked by the VM's epoch), so
    /// the per-pixel cost is a luma dot product and one table lookup.
    remap_lut: Option<alloc::boxed::Box<[[u8; 3]; 256]>>,
    remap_lut_for: u32,
    /// Map mode: this engine runs a *map program* (per-pixel `plot(x, y[, z])`)
    /// and collects coordinates instead of colors. Set via `enable_map_mode`.
    is_map: bool,
    map_coords: Vec<[Fx; 3]>,
    map_dims: u8,
    /// The installed map read as a regular W×H grid, when it is one — the
    /// post-process chain's spatial stages use it to blur in map space
    /// instead of along the wiring (Gitea #140). Six bytes, recomputed only
    /// on [`set_map`]; `None` means "not a grid", and the stages keep their
    /// index-space behavior.
    grid: Option<crate::outpipe::GridMap>,
    /// An `assert()` invariant failed during init: rendering is blocked
    /// for this engine's lifetime (map installs must not resurrect it —
    /// the fix is a config change, which rebuilds the engine).
    requires_violated: bool,
    /// The host's projection defaults (Gitea #473). Only the field matching
    /// the pattern's own dimensionality is ever consulted, and only when it
    /// differs from the Layout's.
    projection: Projection,
    /// [`projection`] resolved against this pattern and this Layout.
    plan: ProjPlan,
    /// Render calls per frame: `pixel_count`, except under
    /// [`ProjPlan::Strip`] where it is the strip length.
    render_count: u32,
    /// The Layout's map while [`ProjPlan::Strip`] has the VM believing it
    /// runs on a bare strip. `Some` ONLY in that state — everywhere else the
    /// Layout map lives in `vm.map`, exactly as before.
    layout_map: Option<MapData>,
    /// One frame of the rendered strip, for the replicate pass. Grown once
    /// when a strip plan is installed (fallibly — a refusal falls back to
    /// by-index), reused every frame: no per-frame and no per-pixel alloc.
    strip_scratch: Vec<[u8; 3]>,
    /// This pattern's `renderFrame` names a coordinate/grid-space bulk op,
    /// so it is a 2D pattern for projection purposes (proposal §5.4d: bulk
    /// patterns follow the 2D row). A `renderFrame` that only paints in
    /// index space is a strip pattern and stays one. Scanned once at load —
    /// [`uses_coordinate_bulk_op`] walks the bytecode.
    frame_is_2d: bool,
    /// This program compiled to native code (Gitea #658,
    /// docs/jit-design.md §6). `Some` means WHOLE-program native: every
    /// entry the engine calls has a native entry point, and the
    /// interpreter is not used for `beforeRender`/`render*`/`renderFrame`
    /// until this is dropped.
    ///
    /// docs/jit-design.md §6 puts this on `Program`. It lives here
    /// instead, deliberately: a [`Program`] is `Clone` and a claim on
    /// executable memory is not, and the ENGINE is the thing whose
    /// lifetime the code has to match anyway (activation builds it,
    /// `drop_prev` frees it). See "§6 as built" in the design.
    #[cfg(feature = "jit")]
    native: Option<crate::jit::NativeProgram>,
}

impl Engine {
    /// Compile and initialize a pattern. Compile errors fail construction;
    /// runtime errors during init are recorded in `last_error` and the
    /// engine stays usable (PB shows vmerr and keeps going).
    #[cfg(feature = "frontend")]
    pub fn new(src: &str, pixel_count: u32, seed: u64) -> Result<Engine, Diagnostic> {
        Ok(Engine::from_program(compile(src)?, pixel_count, seed))
    }

    /// [`new`], but with the wall clock (unix seconds, timezone already
    /// applied) available DURING top-level init. PB patterns may read
    /// `clockHour()`-family builtins at top level — on a real device the
    /// RTC is set by the time a pattern loads, so init sees real time,
    /// never 0. Hosts that know the time should prefer this over pairing
    /// [`new`] with a later [`set_wall_clock`], which reaches only
    /// `beforeRender`/`render` (Gitea #104). `None` = no time source.
    #[cfg(feature = "frontend")]
    pub fn new_at(
        src: &str,
        pixel_count: u32,
        seed: u64,
        wall_unix: Option<i64>,
    ) -> Result<Engine, Diagnostic> {
        Ok(Engine::from_program_budgeted_at(
            compile(src)?,
            pixel_count,
            seed,
            usize::MAX,
            wall_unix,
        ))
    }

    /// Initialize from an already-compiled program (deserialized LXBC
    /// bytecode, or a fresh `compile()` result). Infallible: like `new`
    /// after its compile step, init-time runtime errors land in
    /// `last_error` and the engine stays usable.
    pub fn from_program(prog: Program, pixel_count: u32, seed: u64) -> Engine {
        Engine::from_program_budgeted(prog, pixel_count, seed, usize::MAX)
    }

    /// [`from_program`] with an array-arena BYTE budget, applied BEFORE the
    /// pattern's init code runs (the PB-compat 10,240-element budget always
    /// applies on top). Small-heap devices size this from live free heap so
    /// an array-hungry pattern gets a recorded "array budget" vmerr instead
    /// of exhausting the allocator (which panics = reboots the device — the
    /// soak-v5 lesson).
    pub fn from_program_budgeted(
        prog: Program,
        pixel_count: u32,
        seed: u64,
        array_byte_budget: usize,
    ) -> Engine {
        Engine::from_program_budgeted_at(prog, pixel_count, seed, array_byte_budget, None)
    }

    /// [`from_program_budgeted`] + the wall clock for init, per [`new_at`].
    pub fn from_program_budgeted_at(
        prog: Program,
        pixel_count: u32,
        seed: u64,
        array_byte_budget: usize,
        wall_unix: Option<i64>,
    ) -> Engine {
        Engine::from_program_budgeted_at_ext(
            prog,
            pixel_count,
            seed,
            array_byte_budget,
            crate::vm::DEFAULT_ARRAY_BUDGET,
            wall_unix,
        )
    }

    /// [`from_program_budgeted_at`] with the PB-compat ELEMENT ledger raised
    /// too — for a device whose array arena is not the main heap (Gitea
    /// #253: the Seengreat S3's external PSRAM). Everywhere else, pass
    /// [`crate::vm::DEFAULT_ARRAY_BUDGET`] and nothing changes: the element
    /// ledger is the oracle-bisected PB number and diverging from it is a
    /// deliberate, board-scoped Luxel extension, never a default.
    /// [`crate::vm::MAX_ARENA_SLOTS`] still bounds the arena's slot vector,
    /// which stays on the ordinary allocator whatever the arena does.
    pub fn from_program_budgeted_at_ext(
        prog: Program,
        pixel_count: u32,
        seed: u64,
        array_byte_budget: usize,
        array_element_budget: usize,
        wall_unix: Option<i64>,
    ) -> Engine {
        let mut vm = Vm::new(&prog, seed);
        vm.array_byte_budget = array_byte_budget;
        vm.array_budget = array_element_budget;
        vm.wall_unix = wall_unix;
        vm.globals[prog.pixel_count_g as usize] = Value::Num(Fx::from_int(pixel_count as i32));

        // Sensor-board bindings: exported sensor arrays start zero-filled so
        // sound/motion patterns run dark instead of erroring when no sensor
        // source is attached; a source feeds them via [Engine::set_sensors].
        // Scalars are already 0. The pattern's own init may overwrite these.
        for (name, len) in [
            ("frequencyData", 32),
            ("accelerometer", 3),
            ("analogInputs", 5),
        ] {
            if let Some(i) = prog.global_index(name) {
                if prog.globals[i as usize].export {
                    let mut zeros: crate::arena::ArrVec<Value> = crate::arena::empty();
                    zeros.resize(len, Value::default());
                    if let Ok(v) = vm.alloc_array(zeros) {
                        vm.globals[i as usize] = v;
                    }
                }
            }
        }

        // Run top-level init. `assert()` statements execute inline here —
        // they see everything initialized above them; a failed assert
        // aborts init on the spot (is_assert) and blocks the pattern:
        // its declared configuration invariant doesn't hold, and the fix
        // is a config change, which rebuilds the engine.
        let mut last_error = None;
        if let Err(e) = vm.call(&prog, 0, &[]) {
            last_error = Some(e);
        }
        let violated = last_error.as_ref().is_some_and(|e| e.is_assert);
        // State seeded at top level (`setPixelState` in init) is "the frame
        // before the first frame": hand it over so frame 1 reads it.
        vm.pixel_state_commit();

        vm.pixel_count = pixel_count;
        let before = if violated {
            None
        } else {
            prog.exported_fn("beforeRender")
        };
        let render_tgt = if violated {
            [None; 4]
        } else {
            render_targets(&prog)
        };
        let render = resolve_render(&render_tgt, &vm.globals, 0);

        let mut controls = Vec::new();
        for (name, idx) in &prog.exported_fns {
            for (prefix, kind) in CONTROL_PREFIXES {
                if let Some(label) = name.strip_prefix(prefix) {
                    if !label.is_empty() {
                        controls.push(Control {
                            kind: *kind,
                            label: String::from(label),
                            name: name.clone(),
                            fn_idx: *idx,
                        });
                        break;
                    }
                }
            }
        }

        let mut engine = Engine {
            pixels: alloc::vec![[0u8; 3]; pixel_count as usize],
            prog,
            vm,
            pixel_count,
            before,
            render_tgt,
            render,
            controls,
            time_acc: 0,
            render_time_acc: 0,
            frame_acc: 0,
            rendered_once: false,
            arrays_refused: last_error
                .as_ref()
                .is_some_and(|e| crate::vm::is_array_budget_error(&e.message)),
            last_error,
            debug_enabled: false,
            run_stage: None,
            cur_delta: Fx::ZERO,
            gamma_lut: None,
            gamma_lut_for: Fx::ZERO,
            remap_lut: None,
            remap_lut_for: 0,
            is_map: false,
            map_coords: Vec::new(),
            map_dims: 0,
            grid: None,
            requires_violated: violated,
            projection: Projection::DEFAULT,
            plan: ProjPlan::Native,
            render_count: pixel_count,
            layout_map: None,
            strip_scratch: Vec::new(),
            frame_is_2d: false,
            // Compilation happens AFTER construction, because it needs the
            // init errors this constructor collects (§2.3's exemption is
            // only sound when init ran to completion).
            #[cfg(feature = "jit")]
            native: None,
        };

        // A pattern that renders ONLY in 2D/3D gets a default square-ish
        // grid map rather than 1D fallback coordinates. This is the
        // PB-as-experienced behavior (oracle-verified 2026-07-08): new PBs
        // ship with a default matrix map and a saved map cannot be removed
        // through the public interface, so on a real PB `render2D` always
        // receives genuine map coordinates — and the common
        // `sqrt(pixelCount)`-grid patterns depend on that. A host-installed
        // map replaces this (set_map), exactly like saving a map on a PB.
        // A whole-frame pattern joins that rule when — and only when — it
        // actually asks for coordinates: `renderFrame` + `fillRect` wants
        // the square grid a `render2D` pattern gets, while `renderFrame` +
        // `fillHSV` is a strip pattern and must not be handed a geometry
        // it never mentioned (it would change `pixelMapDimensions()` and
        // the post chain's spatial stages under it).
        engine.frame_is_2d = engine.render_tgt[3].is_some() && engine.uses_coordinate_bulk_op();
        let wants_2d =
            engine.render_tgt[1].is_some() || engine.render_tgt[2].is_some() || engine.frame_is_2d;
        if !violated && engine.render_tgt[0].is_none() && wants_2d {
            engine.set_default_grid_map();
        }
        engine.sync_plan();
        engine
    }

    /// Install the default ceil(√pixelCount)-wide row-major grid map (see
    /// from_program_budgeted for why). Public so hosts that clear a user
    /// map can fall back to the same default.
    #[inline(never)]
    pub fn set_default_grid_map(&mut self) {
        let n = self.pixel_count as usize;
        if n == 0 {
            return;
        }
        // integer ceil(sqrt(n)) without floats
        let mut w = 1usize;
        while w * w < n {
            w += 1;
        }
        let h = n.div_ceil(w);
        // procedural: zero heap, so it can never fail on a starved device
        // the way the 48 KB per-pixel form did on the 64x64 panel (#275/#258)
        self.set_grid_map(w.min(u16::MAX as usize) as u16, h.min(u16::MAX as usize) as u16);
    }

    /// Install a procedural `w`×`h` row-major 2D grid map: coordinates are
    /// computed per pixel, nothing is allocated, and the outpipe's grid-aware
    /// stages (2D blur/glow) see the geometry directly. This is what a
    /// matrix/panel board wants — the rig IS a grid — and what the default
    /// map uses (Gitea #258).
    pub fn set_grid_map(&mut self, w: u16, h: u16) {
        let (w, h) = (w.max(1), h.max(1));
        self.plan_reset();
        self.grid = Some(crate::outpipe::GridMap { w, h, serpentine: false });
        self.vm.frame_grid = self.grid;
        self.vm.map = Some(MapData::grid(w, h));
        if !self.requires_violated {
            self.render = self.resolve_render_now();
        }
        self.sync_plan();
    }

    /// Install the 1D Layout: no map at all, so the pattern's coordinates are
    /// the strip's own `index / pixelCount`. The counterpart of
    /// [`set_grid_map`] for a host whose Layout really is a strip (`strip N`
    /// on `/api/layout`) — without it a 2D-only pattern keeps the fabricated
    /// ceil(√n) grid from construction and the 2D→1D projections (middle
    /// row / middle column) can never come into play.
    pub fn set_strip_layout(&mut self) {
        self.plan_reset();
        self.grid = None;
        self.vm.frame_grid = None;
        self.vm.map = None;
        if !self.requires_violated {
            self.render = self.resolve_render_now();
        }
        self.sync_plan();
    }

    /// Turn this engine into a *map program* runner: [`run_map`] executes its
    /// per-pixel `render(index)` (which calls `plot(x, y[, z])`) and collects
    /// one coordinate per pixel. Debugging works exactly as for a pattern —
    /// the same per-pixel `drive` loop, so breakpoints/stepping just work.
    pub fn enable_map_mode(&mut self) {
        self.plan_reset();
        self.is_map = true;
        // A map program is per-pixel by definition (`render(index)` calling
        // `plot`), so the whole-frame entry is not a candidate here — drop
        // it rather than teach the map pipeline a stage it can never use.
        self.render_tgt[3] = None;
        self.render = self.resolve_render_now();
    }

    /// Run the map program over every pixel, collecting coordinates. Returns
    /// `true` if it suspended at a debug stop (resume with [`debug_step`]);
    /// `false` when the collection finished. Read the result with [`map`].
    pub fn run_map(&mut self) -> bool {
        if self.run_stage.is_some() {
            return true; // already running/paused — drive it with debug_step
        }
        self.map_coords = alloc::vec![[Fx::ZERO; 3]; self.pixel_count as usize];
        self.map_dims = 0;
        if self.render.is_none() {
            self.last_error = Some(VmError {
                message: String::from(
                    "map program must export function render(index) and call plot(x, y)",
                ),
                fn_idx: u16::MAX,
                pc: u32::MAX,
                line: 0,
                col: 0,
                is_assert: false,
            });
            return false;
        }
        if self.pixel_count == 0 {
            return false;
        }
        self.run_stage = Some(RunStage::Pixel(0)); // maps need no beforeRender
        self.drive(None);
        self.run_stage.is_some()
    }

    /// The collected map: dimensionality (2 or 3) and one coordinate per pixel
    /// (pattern units — the consumer's [`set_map`] normalizes them).
    pub fn map(&self) -> (u8, &[[Fx; 3]]) {
        (
            if self.map_dims == 0 { 2 } else { self.map_dims },
            &self.map_coords,
        )
    }

    // ---- native code (Gitea #658, docs/jit-design.md §6) ----

    /// Hand this engine a compiled image of its own program.
    ///
    /// Whole-program or nothing: the caller has already decided that every
    /// function compiled (a [`luxel_jit::Refusal`] is never partial), so
    /// from here on `beforeRender`, `render*` and `renderFrame` are native
    /// calls and the interpreter is the fallback only for the debugger.
    ///
    /// Callers MUST NOT install native code when [`Engine::debug_enabled`]
    /// is set or when init errored — see `jit_eligible`, which is the one
    /// place those preconditions are spelled.
    #[cfg(feature = "jit")]
    pub fn install_native(&mut self, np: crate::jit::NativeProgram) {
        self.native = Some(np);
    }

    /// Drop the compiled image and fall back to the interpreter. Idempotent.
    #[cfg(feature = "jit")]
    pub fn drop_native(&mut self) {
        self.native = None;
    }

    /// Is a compiled image installed AND currently in use?
    ///
    /// False while the debugger is attached: §3.6 makes the debugger step
    /// the interpreter, and `debug_set_enabled(true)` on a live engine is
    /// allowed, so this is a per-call question rather than a load-time one.
    #[cfg(feature = "jit")]
    #[inline]
    pub fn native_active(&self) -> bool {
        self.native.is_some() && !self.debug_enabled && !self.is_map
    }

    /// `(code_bytes, compile_us)` of the installed image — `/api/status`'s
    /// `jit.code_bytes` / `jit.compile_us`.
    #[cfg(feature = "jit")]
    pub fn native_stats(&self) -> Option<(usize, u32)> {
        self.native.as_ref().map(|n| (n.code_bytes, n.compile_us))
    }

    /// Whether this engine may run native code at all, and why not when it
    /// may not (docs/jit-design.md §4a's `jit.reason` vocabulary).
    ///
    /// Two preconditions live here rather than in the firmware so that the
    /// host tests and the device answer them the same way:
    ///
    /// - **`debug`** — the debugger steps the interpreter (§3.6).
    /// - **`init-error`** — §2.3's init exemption is only sound if init ran
    ///   to completion. An aborted init leaves `ArrNum`-annotated globals
    ///   holding `Num(0)`, and unboxing on the strength of those kinds
    ///   would read a number as an array handle.
    ///
    /// `None` = eligible.
    #[cfg(feature = "jit")]
    pub fn jit_ineligible(&self) -> Option<&'static str> {
        if self.debug_enabled {
            return Some("debug");
        }
        if self.last_error.is_some() {
            return Some("init-error");
        }
        None
    }

    /// Everything one pass of native calls needs, assembled once.
    ///
    /// The raw pointers borrow `self`'s fields for exactly as long as the
    /// returned context is used; `err` is a caller-owned slot the helpers
    /// fill (§3.8 — `VmError` owns a `String`, so it cannot live inside a
    /// `repr(C)` struct).
    ///
    /// # Safety
    /// The returned [`JitCtx`] holds pointers into `self` and into `*err`.
    /// It must not outlive either, and nothing may take a `&mut` to
    /// `self.vm` while native code is running through it.
    #[cfg(feature = "jit")]
    unsafe fn native_ctx(&mut self, err: *mut Option<VmError>) -> crate::jit::JitCtx {
        use crate::jit::{JitCtx, CTX_ARGS, STATUS_OK};
        let (stack_limit, fn_table) = match self.native.as_ref() {
            Some(np) => (np.stack_limit, np.entries.as_ptr()),
            None => (0, core::ptr::null()),
        };
        // One `&mut self.vm`, reborrowed as a raw pointer: `vm` and
        // `globals` point into the same object, so taking two `&mut`s in
        // the literal below would be two live mutable borrows of it.
        let vm: *mut Vm = &mut self.vm;
        let globals = (*vm).globals.as_mut_ptr().cast::<crate::vm::ValueRaw>();
        JitCtx {
            vm,
            prog: &self.prog,
            status: STATUS_OK,
            insn_at: 0,
            fn_idx: 0,
            _pad: 0,
            fuel: crate::vm::FUEL as i32,
            stack_limit,
            args: [0; CTX_ARGS],
            err,
            fn_table,
            builtins: crate::jit::BUILTIN_ENTRIES.as_ptr(),
            globals,
        }
    }

    /// One native host entry: reset the budget exactly as
    /// [`crate::vm::Vm::render_pixel`] does, call, and turn a raised
    /// `status` back into the `Err(VmError)` the interpreter would have
    /// returned.
    ///
    /// # Safety
    /// `ctx` must be one built by [`Engine::native_ctx`] for the image
    /// currently installed, and `err` the slot it points at.
    #[cfg(feature = "jit")]
    unsafe fn native_enter(
        np: &crate::jit::NativeProgram,
        ctx: *mut crate::jit::JitCtx,
        fn_idx: u16,
        args: &[i32],
    ) -> Result<(), VmError> {
        let Some((addr, abi)) = np.entry(fn_idx) else {
            // Cannot happen: the image covers every bytecode function.
            // Reported rather than panicked — a panic here takes the render
            // task down on a board with no serial port.
            return Err(VmError {
                message: String::from("native entry missing (JIT bug)"),
                fn_idx,
                pc: 0,
                line: 0,
                col: 0,
                is_assert: false,
            });
        };
        // Every host entry resets the fuel budget (§3.6 / `Vm::call`).
        (*ctx).fuel = crate::vm::FUEL as i32;
        (*ctx).status = crate::jit::STATUS_OK;
        (*ctx).fn_idx = fn_idx;
        (*ctx).insn_at = 0;
        // The error slot is reached only through `ctx.err`: taking a second
        // `&mut` to the caller's local would invalidate the pointer the
        // helpers write through.
        let err = &mut *(*ctx).err;
        *err = None;
        np.call.enter(addr, ctx, abi, args);
        if (*ctx).status == crate::jit::STATUS_OK {
            return Ok(());
        }
        (*ctx).status = crate::jit::STATUS_OK;
        Err((*(*ctx).err).take().unwrap_or_else(|| VmError {
            // A raised status with no error is a helper bug; report it
            // rather than losing the failure.
            message: String::from("native call failed without an error (JIT bug)"),
            fn_idx,
            pc: 0,
            line: 0,
            col: 0,
            is_assert: false,
        }))
    }

    /// Start one of [`drive`]'s stage entries (`beforeRender`,
    /// `renderFrame`) — natively when an image is installed and the
    /// debugger is not attached, through the interpreter otherwise.
    ///
    /// Native code is never resumable, which is exactly why it is off
    /// while debugging: a native stage always returns `Done` or `Err`,
    /// never `Paused`.
    fn start_stage(&mut self, fn_idx: u16, args: &[Value]) -> Result<Outcome, VmError> {
        #[cfg(feature = "jit")]
        if self.native_active() {
            let mut raw = [0i32; 4];
            let n = args.len().min(raw.len());
            for (slot, a) in raw.iter_mut().zip(args) {
                *slot = a.num().raw();
            }
            return self.native_stage(fn_idx, &raw[..n]);
        }
        self.vm.start(&self.prog, fn_idx, args, self.debug_enabled)
    }

    /// One native call standing in for a whole [`drive`] stage
    /// (`beforeRender`, `renderFrame`). The stage entries run once a frame,
    /// so the context is built per call rather than hoisted the way the
    /// per-pixel pass hoists it.
    #[cfg(feature = "jit")]
    fn native_stage(&mut self, fn_idx: u16, args: &[i32]) -> Result<Outcome, VmError> {
        let np: *const crate::jit::NativeProgram =
            self.native.as_ref().expect("native_stage without an image");
        let mut errslot: Option<VmError> = None;
        // SAFETY: `ctx` borrows `self.vm`, `self.prog` and `errslot` for
        // the length of this function and nothing else touches them while
        // the call runs; `np` addresses the image the context was built
        // for, which is not replaced here.
        unsafe {
            let mut ctx = self.native_ctx(&mut errslot);
            Self::native_enter(&*np, &mut ctx, fn_idx, args)
                .map(|()| Outcome::Done(Value::default()))
        }
    }

    // ---- debugger ----

    /// Enable/disable debugging. Disabling abandons any paused run.
    pub fn debug_set_enabled(&mut self, on: bool) {
        if on == self.debug_enabled {
            return;
        }
        self.debug_enabled = on;
        if on {
            self.vm.dbg = Some(DebugState::default());
        } else {
            self.vm.dbg = None;
            self.vm.clear_run();
            self.run_stage = None;
        }
    }

    /// Set breakpoints by 1-based source line; returns the lines that
    /// resolved to code (for gutter feedback). Replaces the previous set.
    /// Install breakpoints by 1-based source line. A line with no
    /// instructions (blank, comment, brace) snaps forward to the nearest
    /// executable line; a line past all code is dropped. Returns the
    /// resolved lines so UIs can move their markers accordingly.
    pub fn debug_set_breakpoints(&mut self, lines: &[u32]) -> Vec<u32> {
        let mut pcs = Vec::new();
        let mut resolved = Vec::new();
        for &line in lines {
            // nearest executable line >= requested (pos entries are runs
            // keyed by fn-relative word index)
            let mut target: Option<u32> = None;
            for f in &self.prog.fns {
                for &(_, l, _) in &f.pos {
                    if l >= line && l != 0 {
                        target = Some(target.map_or(l, |t| t.min(l)));
                    }
                }
            }
            let Some(t) = target else { continue };
            for (fi, f) in self.prog.fns.iter().enumerate() {
                if let Some(&(off, _, _)) = f.pos.iter().find(|&&(_, l, _)| l == t) {
                    pcs.push((fi as u16, off));
                }
            }
            if !resolved.contains(&t) {
                resolved.push(t);
            }
        }
        resolved.sort_unstable();
        if let Some(d) = self.vm.dbg.as_mut() {
            d.breakpoints = pcs;
        }
        resolved
    }

    /// Request a pause at the next instruction of the next/current frame.
    pub fn debug_pause(&mut self) {
        if let Some(d) = self.vm.dbg.as_mut() {
            d.pause_requested = true;
        }
    }

    pub fn debug_paused(&self) -> bool {
        self.run_stage.is_some()
    }

    /// Resume a paused run with the given stepping behavior. Execution flows
    /// across callback boundaries: stepping past the end of render(i) lands
    /// at the top of render(i+1). Returns whether still paused.
    pub fn debug_step(&mut self, kind: StepKind) -> bool {
        if self.run_stage.is_some() {
            self.drive(Some(kind));
        }
        self.run_stage.is_some()
    }

    /// (line, col, pixel-index-if-in-render) of the paused position.
    pub fn debug_location(&self) -> Option<(u32, u32, Option<u32>)> {
        self.run_stage?;
        let f = self.vm.frames().last()?;
        let (line, col) = self.prog.fns[f.fn_idx as usize].pos_at(f.pc);
        let pixel = match self.run_stage {
            Some(RunStage::Pixel(i)) => Some(i),
            _ => None,
        };
        Some((line, col, pixel))
    }

    /// The paused call stack, innermost frame first, with named locals.
    pub fn debug_stack(&self) -> Vec<DebugFrame> {
        let frames = self.vm.frames();
        frames
            .iter()
            .enumerate()
            .rev()
            .map(|(i, f)| {
                let def = &self.prog.fns[f.fn_idx as usize];
                // the top frame's pc is next-to-execute; parents' point past
                // their call instruction
                let pc = if i == frames.len() - 1 {
                    f.pc
                } else {
                    f.pc.saturating_sub(1)
                };
                let (line, col) = def.pos_at(pc);
                let locals = def
                    .local_names
                    .iter()
                    .cloned()
                    .zip(self.vm.frame_locals(f, def.locals as usize).iter().copied())
                    .collect();
                DebugFrame {
                    name: def.name.clone(),
                    fn_idx: f.fn_idx,
                    pc,
                    line,
                    col,
                    locals,
                }
            })
            .collect()
    }

    pub fn pixel_count(&self) -> u32 {
        self.pixel_count
    }

    /// The installed map read as a regular W×H grid, when it is one (see
    /// [`crate::outpipe::detect_grid`]). Hosts that run their own spatial
    /// output stages use this to match the chain's map-aware behavior.
    /// The map currently installed in the VM (a host map, the board grid or
    /// the default grid), if any.
    pub fn installed_map(&self) -> Option<&MapData> {
        self.layout_map()
    }

    /// The Layout's map wherever it currently lives — `vm.map` normally, the
    /// engine's own slot while an along-axis projection has the VM running
    /// on a bare virtual strip.
    fn layout_map(&self) -> Option<&MapData> {
        self.layout_map.as_ref().or(self.vm.map.as_ref())
    }

    /// The Layout's dimensionality: 1 = strip (no map), 2 = matrix or a 2D
    /// map, 3 = a 3D map. Unaffected by any projection in force.
    pub fn layout_dims(&self) -> u8 {
        norm_dims(self.layout_map().map_or(0, |m| m.dims))
    }

    /// The projection defaults this engine holds (Gitea #473).
    pub fn projection(&self) -> Projection {
        self.projection
    }

    /// Install the projection defaults — how a pattern whose dimensionality
    /// differs from the Layout's is shown on it. Only the field matching this
    /// pattern's dimensionality is consulted, and only when it differs from
    /// the Layout's, so the same triple can be carried across pattern and
    /// Layout changes. Takes effect from the next [`Engine::frame`].
    ///
    /// Note `pixelCount` under an along-axis projection becomes the strip's
    /// length, and the pattern's top-level init has already run by then: a
    /// pattern that sizes a buffer with `array(pixelCount)` at the top level
    /// keeps the Layout-sized buffer it allocated. Hosts should install the
    /// projection right after building the engine, before the first frame.
    pub fn set_projection(&mut self, projection: Projection) {
        if projection == self.projection {
            return;
        }
        self.projection = projection;
        self.sync_plan();
    }

    /// The projection actually in force, or `None` when the pattern is
    /// native to the Layout (nothing is being projected).
    pub fn effective_projection(&self) -> Option<ProjectionMode> {
        self.projection
            .effective(self.render_dims(), self.layout_dims())
    }

    /// What the pattern sees this frame: `pixelCount`, its own
    /// dimensionality, the grid its grid-space builtins read, and the
    /// projection in force. Everything a UI needs to caption a tile
    /// (`1D · along x`) or size a preview.
    pub fn effective_geometry(&self) -> EffectiveGeometry {
        let (w, h) = self.vm.frame_grid.map_or((0, 0), |g| (g.w, g.h));
        let (pdims, ldims) = (self.render_dims(), self.layout_dims());
        EffectiveGeometry {
            pixel_count: self.vm.pixel_count,
            pattern_dims: pdims,
            layout_dims: ldims,
            w,
            h,
            mode: self.effective_projection(),
            compatible: crate::projection::compatible(pdims, ldims),
        }
    }

    /// The dimensionality the pattern is CURRENTLY being rendered as — the
    /// RESOLVED entry's, which is what a projection has to bridge. A
    /// `renderFrame` follows the 2D row when it actually draws in grid space
    /// ([`Engine::frame_is_2d`]) and is a strip pattern otherwise — the same
    /// distinction the default-grid rule already makes (proposal §5.4d).
    ///
    /// Not [`pattern_dims`](Self::pattern_dims), which is what the pattern
    /// DECLARES: they differ for a pattern exporting several entries (both
    /// `render` and `render2D` declares 2, but renders 1D on a strip) and
    /// whenever a late-bound entry changes between frames.
    ///
    /// `0` means DIMENSIONLESS — an index-space `renderFrame` that names no
    /// geometry — and is native on every Layout
    /// ([`crate::projection::projection_options`]). It is not 1: a 1D pattern
    /// is a strip drawn on this Layout and can be projected along an axis,
    /// while a whole-frame pattern owns the buffer and never is, so calling it
    /// 1D offered an along-x choice that changed nothing (`library/fairies.js`,
    /// 2026-09-20).
    fn render_dims(&self) -> u8 {
        match self.render {
            Some(RenderKind::R1(_)) => 1,
            Some(RenderKind::R3(_)) => 3,
            Some(RenderKind::R2(_)) => 2,
            Some(RenderKind::Frame(_)) => {
                if self.frame_is_2d {
                    2
                } else {
                    0
                }
            }
            None => self.preferred_dims(),
        }
    }

    /// Undo whatever the current plan changed about the VM's view of the
    /// world, leaving the Layout installed exactly as a projection-less
    /// engine would have it. Every plan transition goes through here, so no
    /// plan needs its own inverse.
    fn plan_reset(&mut self) {
        if self.plan == ProjPlan::Native {
            return;
        }
        if let Some(m) = self.layout_map.take() {
            self.vm.map = Some(m);
        }
        self.vm.frame_grid = self.grid;
        self.set_vm_pixel_count(self.pixel_count);
        self.render_count = self.pixel_count;
        self.plan = ProjPlan::Native;
    }

    /// Recompute the plan for (this pattern, this Layout, this projection)
    /// and install it if it changed. Cheap and idempotent — called from every
    /// map install, from `set_projection`, and once per frame after the
    /// render entry is re-resolved (it can be late-bound).
    fn sync_plan(&mut self) {
        let want = self.compute_plan();
        if want == self.plan {
            return;
        }
        self.plan_reset();
        match want {
            ProjPlan::Strip { len, .. } => {
                // One buffer for the rendered strip. Fallible: a device that
                // cannot spare it keeps by-index rather than dying.
                self.strip_scratch.clear();
                if self.strip_scratch.capacity() < len as usize
                    && self
                        .strip_scratch
                        .try_reserve_exact(len as usize)
                        .is_err()
                {
                    return;
                }
                // The pattern is a strip of `len` for as long as this plan
                // stands: no map, `pixelCount` = len, no grid.
                self.layout_map = self.vm.map.take();
                self.vm.frame_grid = None;
                self.set_vm_pixel_count(len);
                self.render_count = len;
            }
            ProjPlan::FrameGrid(g) => self.vm.frame_grid = Some(g),
            ProjPlan::Native | ProjPlan::Select(_) => {}
        }
        self.plan = want;
    }

    fn set_vm_pixel_count(&mut self, n: u32) {
        if self.vm.pixel_count == n {
            return;
        }
        self.vm.pixel_count = n;
        self.vm.globals[self.prog.pixel_count_g as usize] = Value::Num(Fx::from_int(n as i32));
    }

    /// One cell of the §5.4d table, resolved to something executable.
    fn compute_plan(&self) -> ProjPlan {
        if self.is_map || self.requires_violated {
            return ProjPlan::Native;
        }
        let Some(render) = self.render else {
            return ProjPlan::Native;
        };
        let is_frame = matches!(render, RenderKind::Frame(_));
        let pdims = self.render_dims();
        let ldims = self.layout_dims();
        if is_frame {
            // A whole-frame pattern owns the buffer: there is no per-pixel
            // strip to render and replicate, and no coordinate argument to
            // substitute. That is why an index-space one is `render_dims() ==
            // 0` (dimensionless) rather than 1 — there is no projection for a
            // host to offer it. The one thing it can need is a grid — a grid-space
            // `renderFrame` on a 1D Layout is an incompatible pairing no host
            // offers (#538), but if one is activated anyway it gets a w×1
            // grid so `gridWidth`/`gridHeight` and the grid-space bulk
            // builtins describe the strip instead of nothing.
            if pdims == 2 && ldims == 1 {
                let n = self.pixel_count.min(u16::MAX as u32) as u16;
                return ProjPlan::FrameGrid(crate::outpipe::GridMap {
                    w: n,
                    h: 1,
                    serpentine: false,
                });
            }
            return ProjPlan::Native;
        }
        let Some(mode) = self.projection.effective(pdims, ldims) else {
            // Native, or an incompatible pairing: `pixel_coords` fills the
            // axes the Layout does not have with mid-space, which is what an
            // incompatible pattern renders on.
            return ProjPlan::Native;
        };
        if pdims == 1 {
            // 1D pattern on a 2D/3D Layout. By index changes nothing, so it
            // keeps today's path.
            let Some(axis) = mode.axis() else {
                return ProjPlan::Native;
            };
            let len = self.axis_len(axis);
            return if len == 0 {
                ProjPlan::Native
            } else {
                ProjPlan::Strip { axis, len }
            };
        }
        // The only row left is a 2D pattern on a lattice: the xy image
        // extruded along an axis, so the pattern's two coordinates are the
        // other two. `sel[k]` picks the Layout coordinate (0/1/2, or 3 for
        // mid-space) feeding the pattern's k-th coordinate argument.
        match (pdims, ldims, mode) {
            (2, 3, ProjectionMode::X) => ProjPlan::Select([1, 2, 3]),
            (2, 3, ProjectionMode::Y) => ProjPlan::Select([0, 2, 3]),
            // `repeat along z` is what `pixel_coords` already does
            _ => ProjPlan::Native,
        }
    }

    /// How many distinct cells the Layout has along `axis` — the length of
    /// the strip an along-axis projection renders, and what `pixelCount`
    /// then reads.
    ///
    /// Known exactly for a procedural W×H grid (the panel and matrix case,
    /// where the 64× saving lives) and for a coordinate map that
    /// [`crate::outpipe::detect_grid`] recognised. Any other map — an
    /// irregular 2D cloud, any 3D Layout — has no cell count, so the strip is
    /// as long as the Layout and pixels sample it by coordinate: the same
    /// picture, without the saving.
    fn axis_len(&self, axis: u8) -> u32 {
        let Some(m) = self.layout_map() else {
            return 0;
        };
        if let Some((w, h)) = m.grid {
            return match axis {
                0 => w as u32,
                1 => h as u32,
                _ => 0,
            };
        }
        if let Some(g) = self.grid.filter(|g| g.len() == m.coords.len()) {
            // GridMap's `w` counts the run the INDEX walks first, which is
            // whichever coordinate axis moves between pixel 0 and pixel 1.
            let fast = match (m.coords.first(), m.coords.get(1)) {
                (Some(a), Some(b)) if a[0] != b[0] => 0u8,
                (Some(a), Some(b)) if a[1] != b[1] => 1u8,
                _ => return self.pixel_count,
            };
            return match axis {
                0 | 1 if axis == fast => g.w as u32,
                0 | 1 => g.h as u32,
                _ => 0,
            };
        }
        self.pixel_count
    }

    /// Spread the strip rendered into `pixels[..len]` across the whole
    /// Layout along the projection's axis. Reads each pixel's Layout
    /// coordinate, so a serpentine panel, a rotated map or an irregular
    /// cloud all replicate correctly — no wiring assumptions.
    fn project_replicate(&mut self) {
        let ProjPlan::Strip { axis, len } = self.plan else {
            return;
        };
        let (n, len) = (self.pixel_count as usize, len as usize);
        if len == 0 || n == 0 || self.pixels.len() < len {
            return;
        }
        self.strip_scratch.clear();
        self.strip_scratch.extend_from_slice(&self.pixels[..len]);
        let last = (len - 1) as u32;
        let axis = axis as usize;
        for i in 0..n {
            let j = match &self.layout_map {
                Some(m) if last > 0 => {
                    let v = m.coord(i)[axis].raw().clamp(0, 65_535) as u32;
                    (((v * last) + 32_767) / 65_535) as usize
                }
                _ => 0,
            };
            self.pixels[i] = self.strip_scratch[j.min(len - 1)];
        }
    }

    pub fn grid(&self) -> Option<crate::outpipe::GridMap> {
        self.grid
    }

    /// The compiled program this engine runs (e.g. for [`crate::bytecode::serialize`]).
    pub fn program(&self) -> &Program {
        &self.prog
    }

    /// Dynamic opcode counters for everything this engine has executed
    /// (`profile` feature; host tooling — see `luxel bench --profile`).
    #[cfg(feature = "profile")]
    pub fn profile(&self) -> &crate::vm::prof::Profile {
        self.vm.profile()
    }

    /// Zero the counters — the CLI calls it after init so the numbers
    /// describe the render pass.
    #[cfg(feature = "profile")]
    pub fn profile_reset(&mut self) {
        self.vm.profile_reset();
    }

    /// Install a pixel map: one coordinate tuple per pixel, any units.
    /// Coordinates normalize per-axis into world units 0..1 (exclusive —
    /// quantized to u16/65536 like PB's map binary). Render selection
    /// re-picks by map dimensionality.
    ///
    /// Returns `false` — and leaves the engine's map untouched — when the
    /// per-pixel buffer cannot be allocated. At 4096 px a map is 48 KB, and
    /// a new engine is built while the outgoing pattern still holds its
    /// heap: an infallible `vec!` here panicked (reboot) on every pattern
    /// swap after a heavy pattern on the 64x64 panel (Gitea #275).
    pub fn set_map(&mut self, dims: u8, raw: &[[Fx; 3]]) -> bool {
        let n = (self.pixel_count as usize).min(raw.len());
        let mut coords: Vec<[Fx; 3]> = Vec::new();
        if coords.try_reserve_exact(n).is_err() {
            return false;
        }
        coords.extend_from_slice(&raw[..n]);
        self.set_map_vec(dims, coords)
    }

    /// [`set_map`] for a caller that already owns the buffer: normalizes in
    /// place, so installing a map costs one per-pixel allocation instead of
    /// two (the raw copy plus the normalized one). This is what the default
    /// grid map uses. Never allocates; always returns `true`.
    pub fn set_map_vec(&mut self, dims: u8, mut coords: Vec<[Fx; 3]>) -> bool {
        let n = (self.pixel_count as usize).min(coords.len());
        coords.truncate(n);
        self.plan_reset();
        // grid detection wants the raw (pattern-unit) coordinates
        self.grid = crate::outpipe::detect_grid(dims, &coords);
        self.vm.frame_grid = self.grid;
        for axis in 0..(dims as usize).min(3) {
            let mut min = i64::MAX;
            let mut max = i64::MIN;
            for c in coords.iter() {
                min = min.min(c[axis].raw() as i64);
                max = max.max(c[axis].raw() as i64);
            }
            let span = max - min;
            for c in coords.iter_mut() {
                let v = if span == 0 {
                    0
                } else {
                    ((c[axis].raw() as i64 - min) * 65_535 + span / 2) / span
                };
                c[axis] = Fx::from_raw(v as i32);
            }
        }
        self.vm.map = Some(MapData { dims, coords, grid: None });
        if !self.requires_violated {
            self.render = self.resolve_render_now();
        }
        self.sync_plan();
        true
    }

    /// [`resolve_render`] against the LAYOUT's dims and the global values.
    /// The Layout, not `vm.map`: an along-axis projection hides the map from
    /// the VM for the duration, and entry selection must not flip-flop with
    /// it (see [`Engine::layout_map`]).
    fn resolve_render_now(&self) -> Option<RenderKind> {
        let dims = self.layout_map().map_or(0, |m| m.dims);
        resolve_render(&self.render_tgt, &self.vm.globals, dims)
    }

    /// Provide wall-clock time (unix seconds, timezone already applied) for
    /// the clock builtins.
    pub fn set_wall_clock(&mut self, unix_seconds: i64) {
        self.vm.wall_unix = Some(unix_seconds);
    }

    pub fn controls(&self) -> &[Control] {
        &self.controls
    }

    /// Invoke a control function with values (slider: 1 value, pickers: 3,
    /// trigger: 0). Output controls (showNumber/gauge) return their value.
    pub fn set_control(&mut self, name: &str, values: &[Fx]) -> Option<Fx> {
        let ctl = self
            .controls
            .iter()
            .find(|c| c.name == name || c.label == name)?;
        let fn_idx = ctl.fn_idx;
        let mut args = [Value::default(); 4];
        for (i, v) in values.iter().take(4).enumerate() {
            args[i] = Value::Num(*v);
        }
        match self
            .vm
            .call(&self.prog, fn_idx, &args[..values.len().min(4)])
        {
            Ok(v) => Some(v.num()),
            Err(e) => {
                self.last_error = Some(e);
                None
            }
        }
    }

    /// The geometry the COMPILED pattern asks for, independent of whatever
    /// map a host later installs: `0` = a strip (only `render`, or a
    /// `renderFrame` that draws in index space), `2` = a 2D grid (`render2D`,
    /// or `renderFrame` plus a coordinate/grid-space bulk op), `3` = a 3D
    /// point cloud (`render3D` and nothing 2D).
    ///
    /// These are the same signals `from_program_budgeted` uses to decide
    /// whether to install the default square grid map — exposed so a host can
    /// pick a default preview rig from the source instead of from a manifest
    /// (Gitea #372). 2D wins over 3D, matching the playground gallery's own
    /// `kind`. Note this ignores `render`: a pattern exporting both `render`
    /// and `render2D` counts as 2D here, because a user who wrote `render2D`
    /// meant to see it — the engine's own default-map rule is stricter,
    /// because installing a map there would change what the pattern renders.
    pub fn preferred_dims(&self) -> u8 {
        if self.render_tgt[1].is_some()
            || (self.render_tgt[3].is_some() && self.uses_coordinate_bulk_op())
        {
            2
        } else if self.render_tgt[2].is_some() {
            3
        } else {
            0
        }
    }

    /// The dimensionality the pattern DECLARES, as the UI reports it:
    /// 0 (no preference — a `renderFrame`-only pattern that never asks for
    /// coordinates), 1 (`render`), 2 (`render2D`), 3 (`render3D`).
    ///
    /// This is [`preferred_dims`](Self::preferred_dims) with the 1D case
    /// separated out of its `0`: `preferred_dims` answers "does this pattern
    /// want a map installed", where `render` and `renderFrame` are the same
    /// answer, while `/api/status`'s `pattern_dims` answers "what shape was
    /// this pattern written for", where they are not — a 1D pattern on a
    /// panel is projected and captioned, a dimensionless one is not.
    pub fn pattern_dims(&self) -> u8 {
        match self.preferred_dims() {
            0 if self.render_tgt[0].is_some() => 1,
            d => d,
        }
    }

    /// True if the pattern binds any sensor-board variable — callers use it
    /// to decide whether capturing/forwarding sensor data is worth anything.
    pub fn wants_sensors(&self) -> bool {
        [
            "frequencyData",
            "energyAverage",
            "maxFrequencyMagnitude",
            "maxFrequency",
            "light",
            "accelerometer",
            "analogInputs",
        ]
        .iter()
        .any(|n| {
            self.prog
                .global_index(n)
                .is_some_and(|i| self.prog.globals[i as usize].export)
        })
    }

    /// Inject one frame of sensor data into the exported sensor bindings
    /// (PB sensor-board surface, ~40 Hz on real hardware). Bindings the
    /// pattern doesn't export are skipped; array writes go into whatever
    /// array the exported name currently references.
    pub fn set_sensors(&mut self, s: &SensorFrame) {
        for (name, v) in [
            ("energyAverage", s.energy_average),
            ("maxFrequencyMagnitude", s.max_frequency_magnitude),
            ("maxFrequency", s.max_frequency),
            ("light", s.light),
        ] {
            self.set_var(name, v);
        }
        self.set_sensor_array("frequencyData", &s.frequency_data);
        self.set_sensor_array("accelerometer", &s.accelerometer);
        self.set_sensor_array("analogInputs", &s.analog_inputs);
    }

    fn set_sensor_array(&mut self, name: &str, vals: &[Fx]) {
        let Some(Value::Arr(id)) = self.var(name) else {
            return;
        };
        if let Some(arr) = self.vm.array_mut(&self.prog, id) {
            for (dst, v) in arr.iter_mut().zip(vals) {
                *dst = Value::Num(*v);
            }
        }
    }

    /// Queue an external event `[type, x, y, value]` for the pattern to
    /// read via `readEvent` (HTTP/websocket injection surface — keyboards,
    /// MQTT/HA, sensors). Bounded at [`crate::vm::MAX_EVENTS`], dropping
    /// the OLDEST when full so the freshest input wins. The one-time queue
    /// allocation is fallible: on a heap-starved device the event is
    /// silently dropped rather than erroring — events are best-effort
    /// input, like sensor frames.
    pub fn push_event(&mut self, ev: [Fx; 4]) {
        let q = &mut self.vm.events;
        if q.capacity() < crate::vm::MAX_EVENTS
            && q.try_reserve(crate::vm::MAX_EVENTS - q.len()).is_err()
        {
            return;
        }
        while q.len() >= crate::vm::MAX_EVENTS {
            q.pop_front();
        }
        q.push_back(ev);
    }

    /// Drive a digital input pin from outside the pattern, so `digitalRead`
    /// reports what a host says the wire is doing instead of the pin's idle
    /// level (Gitea #177 item 2). `Some(true)`/`Some(false)` = HIGH/LOW,
    /// `None` releases the pin back to its `pinMode` idle level.
    ///
    /// Returns false for a pin outside `0..=`[`crate::vm::MAX_TRACKED_PIN`],
    /// which has nowhere to store the state — callers surface that rather
    /// than letting a typo'd pin look like a stuck input.
    ///
    /// Injected levels live on the VM, so they last until the pin is released
    /// or the pattern is recompiled (a pattern switch rebuilds the VM and
    /// clears them, exactly like the event queue).
    pub fn set_pin(&mut self, pin: i32, level: Option<bool>) -> bool {
        self.vm.set_pin(pin, level)
    }

    /// The level `digitalRead(pin)` currently reports — injected level when
    /// the pin is driven, otherwise its `pinMode` idle level.
    pub fn pin_read(&self, pin: i32) -> bool {
        self.vm.pin_read(pin)
    }

    /// Bit per pin (0..63): pins the pattern has actually named in a
    /// `pinMode`/`digitalRead` so far (Gitea #205). Pin numbers are runtime
    /// values, not statics, so this is the only way a host can tell which
    /// pins are worth offering a control for — the playground shows its pin
    /// panel only for these, and hides it entirely when the mask is empty.
    pub fn pins_used(&self) -> u64 {
        self.vm.pins_used()
    }

    /// Bit per pin (0..63): pins that idle HIGH (a `pinMode` pull-up), i.e.
    /// what `digitalRead` reports while nothing drives them. A host uses it
    /// to work out which direction "pressing" the pin means.
    pub fn pins_idle_high(&self) -> u64 {
        self.vm.pins_idle_high()
    }

    /// The last `pinMode` value for `pin` (Arduino/ESP32 bits: 1 INPUT,
    /// 2 OUTPUT, 4 pull-up, 8 pull-down, 16 open-drain; 0 = never
    /// configured). What the firmware configures the real pad from
    /// (Gitea #177 item 4).
    pub fn pin_mode(&self, pin: i32) -> u8 {
        self.vm.pin_mode(pin)
    }

    /// Bit per pin (0..63): the level the pattern last `digitalWrite`d. The
    /// engine records it; a host drives the pad. Pins never written are LOW.
    pub fn pins_out_high(&self) -> u64 {
        self.vm.pins_out_high()
    }

    /// Drive an analog input pin from outside the pattern, so
    /// `analogRead(pin)` / `touchRead(pin)` report an injected value instead
    /// of the flat 0 they read while nothing drives them (Gitea #206).
    /// `value` is clamped to 0..1, the range both builtins report; writing
    /// [`Fx::ZERO`] releases the pin back to its undriven reading.
    ///
    /// Returns false for a pin outside `0..=`[`crate::vm::MAX_TRACKED_PIN`],
    /// the same out-of-window contract as [`Engine::set_pin`]. Injected
    /// values live on the VM, so a pattern switch clears them.
    pub fn set_analog_pin(&mut self, pin: i32, value: Fx) -> bool {
        self.vm.set_analog_pin(pin, value)
    }

    /// The value `analogRead(pin)` / `touchRead(pin)` currently report.
    pub fn analog_read(&self, pin: i32) -> Fx {
        self.vm.analog_read(pin)
    }

    /// Bit per pin (0..63): pins the pattern has actually named in an
    /// `analogRead`/`touchRead` so far (Gitea #206) — the analog counterpart
    /// of [`Engine::pins_used`], and what a host gates an analog slider on.
    pub fn analog_pins_used(&self) -> u64 {
        self.vm.analog_pins_used()
    }

    /// Read an exported variable.
    pub fn var(&self, name: &str) -> Option<Value> {
        let i = self.prog.global_index(name)?;
        if !self.prog.globals[i as usize].export {
            return None;
        }
        Some(self.vm.globals[i as usize])
    }

    /// Write an exported variable (the `setVars` surface).
    pub fn set_var(&mut self, name: &str, value: Fx) -> bool {
        match self.prog.global_index(name) {
            Some(i) if self.prog.globals[i as usize].export => {
                self.vm.globals[i as usize] = Value::Num(value);
                true
            }
            _ => false,
        }
    }

    /// Exported variable names (the var-watcher surface).
    pub fn exported_vars(&self) -> impl Iterator<Item = &str> {
        self.prog
            .globals
            .iter()
            .filter(|g| g.export)
            .map(|g| g.name.as_str())
    }

    /// All user-defined globals with current values (debugger scope pane —
    /// implicit assignments create globals, so this is where most pattern
    /// state lives). Predefined constants are filtered out.
    pub fn debug_globals(&self) -> Vec<(String, Value)> {
        self.prog
            .globals
            .iter()
            .enumerate()
            .filter(|(_, g)| !g.predefined)
            .map(|(i, g)| (g.name.clone(), self.vm.globals[i]))
            .collect()
    }

    /// Length of a VM array by id (debugger display).
    pub fn array_len(&self, id: u32) -> usize {
        self.vm.array(&self.prog, id).map(|a| a.len()).unwrap_or(0)
    }

    /// Live array-arena occupancy: `(slots, elements, bytes)`. Arrays are
    /// never freed within a Vm, so all three only grow; the element ledger
    /// (and its per-array header) is what bounds them — see
    /// `ARRAY_HEADER_UNITS`.
    pub fn arena_stats(&self) -> (usize, usize, usize) {
        (
            self.vm.arena_slots(),
            self.vm.arena_elems(),
            self.vm.arena_bytes(),
        )
    }

    /// Read an element of an exported array variable.
    pub fn var_array(&self, name: &str) -> Option<ArrView<'_>> {
        match self.var(name)? {
            Value::Arr(id) => self.vm.array(&self.prog, id),
            _ => None,
        }
    }

    /// The engine clock in whole ms (what `time()`/`beat` run on) — the
    /// Luxel-to-Luxel sync surface, together with [Engine::set_time_ms].
    pub fn time_ms(&self) -> u64 {
        self.time_acc >> 16
    }

    /// Hard-set the engine clock (sync convergence when the offset is too
    /// big to slew; small offsets are corrected by stretching `frame`'s
    /// delta instead, which stays smooth).
    pub fn set_time_ms(&mut self, ms: u64) {
        self.time_acc = ms << 16;
        // a clock jump is not a frame delta: don't hand the sync step to
        // beforeRender as elapsed time
        self.render_time_acc = self.time_acc;
        self.vm.time_ms = ms;
    }

    /// Advance time by `delta_ms` and render one frame.
    ///
    /// Two in-pattern controls shape this (both default to off, so an
    /// untouched pattern behaves exactly as before):
    ///
    /// - `timeScale(s)` scales `delta_ms` before it advances the clock, so
    ///   the whole pattern-visible time base — `time()`, `beat()`, the
    ///   `beforeRender` delta — runs at s × real time.
    /// - `setFrameRate(fps)` holds the previous frame (returning the same
    ///   pixels, running no pattern code) until 1000/fps ms of *real* time
    ///   have accumulated. The clock itself keeps running, so `time_ms()`
    ///   stays continuous for Luxel-to-Luxel sync; when the frame does run,
    ///   `beforeRender` gets the whole interval as its delta. The overshoot
    ///   carries into the next period (Gitea #384), so the long-run average
    ///   is the requested rate rather than the caller's tick rate divided by
    ///   a whole number; individual periods still jitter by up to one tick.
    pub fn frame(&mut self, delta_ms: Fx) -> &[[u8; 3]] {
        if self.run_stage.is_some() {
            // paused at a debug stop mid-frame: time frozen, pixels as-is
            return &self.pixels;
        }
        let real = delta_ms.raw().max(0) as u64;
        // Fx::mul wraps on overflow; scale on the raw i64 product instead
        // and clamp, so a big timeScale saturates rather than going negative.
        let scaled = if self.vm.time_scale == Fx::ONE {
            real
        } else {
            ((real as i64 * self.vm.time_scale.raw() as i64) >> 16).min(i32::MAX as i64) as u64
        };
        self.time_acc += scaled;
        self.vm.time_ms = self.time_acc >> 16;
        self.frame_acc = self.frame_acc.saturating_add(real);
        if self.rendered_once && self.vm.frame_min_raw > self.frame_acc {
            return &self.pixels; // under the frame-rate cap: hold this frame
        }
        // Carry the remainder rather than dropping it (Gitea #384). Zeroing
        // the accumulator quantized the achievable rate to the CALLER's tick
        // rate over an integer: against the firmware's 8 ms render loop,
        // `setFrameRate(100)` fired every other tick and delivered 62.5.
        // Subtracting the period makes the long-run average exactly the cap
        // for any cap at or below the tick rate; individual periods still
        // jitter by up to one tick, which is inherent to a discrete loop.
        // The carry is clamped to one period so a long stall (a flash write,
        // a pattern swap) buys at most one catch-up frame instead of a burst
        // of them. `frame_min_raw == 0` (uncapped) clamps to 0, i.e. resets.
        self.frame_acc = self
            .frame_acc
            .saturating_sub(self.vm.frame_min_raw)
            .min(self.vm.frame_min_raw);
        self.rendered_once = true;
        self.cur_delta =
            Fx::from_raw((self.time_acc - self.render_time_acc).min(i32::MAX as u64) as i32);
        self.render_time_acc = self.time_acc;
        self.run_stage = Some(RunStage::Before);
        self.drive(None);
        &self.pixels
    }

    /// Advance the (resumable) frame pipeline until it finishes the frame or
    /// suspends at a debug stop. `resume` continues a paused VM run with the
    /// given step plan; None starts the next stage fresh.
    fn drive(&mut self, mut resume: Option<StepKind>) {
        loop {
            let Some(stage) = self.run_stage else { return };
            let outcome = if let Some(k) = resume.take() {
                if stage == RunStage::Frame {
                    self.frame_buffer_out();
                }
                self.vm.resume(&self.prog, k)
            } else {
                match stage {
                    RunStage::Frame => {
                        let Some(RenderKind::Frame(f)) = self.render else {
                            self.run_stage = None;
                            return;
                        };
                        // The brush (`Vm::pixel`, written by hsv()/rgb()/
                        // paint()) starts every frame black, so a shape op
                        // before the first colour call paints black rather
                        // than last frame's leftover.
                        self.vm.pixel = [Fx::ZERO; 3];
                        self.vm.pixel_written = false;
                        self.frame_buffer_out();
                        // The frame-buffer lend is unchanged by native code
                        // (§6): the bulk ops are builtins and reach
                        // `Vm::frame` through the same helpers.
                        self.start_stage(f, &[])
                    }
                    RunStage::Before => match self.before {
                        Some(b) => self.start_stage(b, &[Value::Num(self.cur_delta)]),
                        None => Ok(Outcome::Done(Value::default())),
                    },
                    RunStage::Pixel(i) => {
                        let Some(render) = self.render else {
                            self.pixels.iter_mut().for_each(|p| *p = [0; 3]);
                            self.run_stage = None;
                            return;
                        };
                        if !self.debug_enabled && !self.is_map {
                            // the common case: no debugger, a color frame —
                            // run every remaining pixel in one tight loop
                            // instead of one trip through this state machine
                            // per pixel (Gitea #260)
                            self.render_pixels(render, i);
                            return;
                        }
                        self.vm.pixel = [Fx::ZERO; 3];
                        self.vm.pixel_written = false;
                        self.vm.plot_coord = [Fx::ZERO; 3];
                        self.vm.plot_dims = 0;
                        self.vm.plot_written = false;
                        let (fn_idx, args, argc) = self.render_args(render, i);
                        self.vm
                            .start(&self.prog, fn_idx, &args[..argc], self.debug_enabled)
                    }
                }
            };
            // One restore point for every exit of the whole-frame stage —
            // Done, Paused and Err alike. The Vec must never be lost and
            // `pixels()` must never be observed empty.
            if stage == RunStage::Frame {
                self.frame_buffer_in();
            }
            let outcome = match outcome {
                Err(e) => {
                    let fatal = e.is_assert || e.is_resource_guard();
                    // first error wins until read: a per-pixel error would
                    // otherwise re-alloc its message for every pixel of
                    // every frame, and the root cause is the earliest one.
                    // `arrays_refused` extends that across frames: the root
                    // cause is then the load-time refusal (Gitea #420).
                    if (self.last_error.is_none() || fatal) && !self.arrays_refused {
                        self.last_error = Some(e);
                    }
                    if fatal {
                        // asserts and VM resource guards stay frame-fatal:
                        // blank the rest of the frame, move on
                        match self.run_stage {
                            Some(RunStage::Pixel(i)) => self.blank_from(i),
                            // a whole-frame handler owns the whole frame
                            Some(RunStage::Frame) => {
                                self.pixels.iter_mut().for_each(|p| *p = [0; 3])
                            }
                            _ => {}
                        }
                        self.run_stage = None;
                        return;
                    }
                    // PB blast radius (oracle fw 3.67, tools/oracle/
                    // oob-probes.mjs): a runtime error aborts only the
                    // current handler invocation — after a beforeRender
                    // abort the pixel pass still runs, and an erroring
                    // render(i) keeps its pre-error hsv() and doesn't stop
                    // later pixels. Fall through as if the handler returned;
                    // vm.pixel already holds whatever was set pre-abort.
                    Ok(Outcome::Done(Value::default()))
                }
                ok => ok,
            };
            match outcome {
                Err(_) => unreachable!("fatal errors return above"),
                Ok(Outcome::Paused) => return,
                Ok(Outcome::Done(_)) => match stage {
                    RunStage::Before => {
                        // Late-bound entries (`export var render2D` assigned
                        // inside beforeRender) resolve now, each frame — and
                        // the projection follows whatever they resolved to.
                        self.render = self.resolve_render_now();
                        self.sync_plan();
                        if self.render.is_none() {
                            self.pixels.iter_mut().for_each(|p| *p = [0; 3]);
                            self.finish_frame();
                            return;
                        }
                        if self.pixel_count == 0 {
                            self.finish_frame();
                            return;
                        }
                        self.run_stage = Some(match self.render {
                            Some(RenderKind::Frame(_)) => RunStage::Frame,
                            _ => RunStage::Pixel(0),
                        });
                    }
                    RunStage::Frame => {
                        // Same tail as the per-pixel pass: the post chain
                        // and the end-of-frame hand-over still run.
                        self.post_chain();
                        self.finish_frame();
                        return;
                    }
                    RunStage::Pixel(i) => {
                        if self.is_map {
                            // map mode: keep the plotted coordinate, not a color
                            if let Some(slot) = self.map_coords.get_mut(i as usize) {
                                *slot = self.vm.plot_coord;
                            }
                            self.map_dims = self.map_dims.max(self.vm.plot_dims);
                        } else {
                            let [r, g, b] = self.vm.pixel;
                            self.pixels[i as usize] = [quantize(r), quantize(g), quantize(b)];
                        }
                        if i + 1 < self.render_count {
                            self.run_stage = Some(RunStage::Pixel(i + 1));
                        } else {
                            self.project_replicate();
                            self.post_chain();
                            self.finish_frame();
                            return;
                        }
                    }
                },
            }
        }
    }

    /// The per-pixel pass without the resumable state machine: same
    /// semantics as [`drive`]'s `Pixel` stage (first error wins, fatal errors
    /// blank the rest of the frame, non-fatal ones keep the pre-error color),
    /// minus the per-pixel outcome plumbing. Only for non-debug, non-map runs.
    fn render_pixels(&mut self, render: RenderKind, from: u32) {
        // Everything about the call that does not change per pixel is
        // resolved once here: the entry, its argument count, whether the
        // coordinate work is needed at all, and the callee's frame shape
        // (Gitea #260 — for an empty render this was the whole frame cost).
        let (fn_idx, argc) = match render {
            RenderKind::R1(f) => (f, 2),
            RenderKind::R2(f) => (f, 3),
            RenderKind::R3(f) => (f, 4),
            // the whole-frame entry never reaches the per-pixel pass
            // (`drive` routes it to RunStage::Frame); keep this total
            // rather than panicking on a state that cannot arise.
            RenderKind::Frame(_) => {
                self.post_chain();
                self.finish_frame();
                return;
            }
        };
        let mid = Fx::from_raw(1 << 15); // 0.5, mid-space fill for missing dims
        // a plain render(index) never reads x: skip the per-pixel coordinate
        // divide (it is a ROM call on Xtensa) entirely
        let index_only = match render {
            RenderKind::R1(f) => self.prog.fns[f as usize].params < 2,
            _ => false,
        };
        // Native code replaces `render_pixel` and nothing else: the
        // coordinate work, the projection, the brush read-back, the
        // first-error rule and the frame tail below are shared, so there is
        // one per-pixel pass and not two (docs/jit-design.md §6).
        //
        // The context is built ONCE per pass — it is 180 bytes, most of it
        // the argument handoff area, and rebuilding it per pixel would put
        // a memset back on the path #260 took one off.
        #[cfg(feature = "jit")]
        let mut errslot: Option<VmError> = None;
        #[cfg(feature = "jit")]
        let native: Option<(*const crate::jit::NativeProgram, crate::jit::JitCtx)> =
            if self.native_active() {
                let np: *const crate::jit::NativeProgram =
                    self.native.as_ref().expect("native_active");
                // SAFETY: the context borrows `self.vm`, `self.prog` and
                // `errslot` as raw pointers for the length of this
                // function; nothing below takes a conflicting reference to
                // `self.vm` while native code is running, and the image
                // `np` points at is not touched until the pass ends.
                let ctx = unsafe { self.native_ctx(&mut errslot) };
                Some((np, ctx))
            } else {
                None
            };
        #[cfg(feature = "jit")]
        let mut native = native;
        // `begin_pixel_pass` also clears any suspended run, which is what
        // makes a swap from a paused interpreter run into a native pass
        // safe, so it runs on both paths; native code ignores the plan.
        let plan = self.vm.begin_pixel_pass(&self.prog, fn_idx, argc);
        // A coordinate-substituting projection (§5.4d) is a loop-invariant
        // 3-byte selector; a strip projection needs nothing here, because the
        // VM has already been told it runs on a bare strip of `render_count`.
        let sel = match self.plan {
            ProjPlan::Select(s) => Some(s),
            _ => None,
        };
        let mut args = [
            Value::Num(Fx::ZERO),
            Value::Num(mid),
            Value::Num(mid),
            Value::Num(mid),
        ];
        for i in from..self.render_count {
            self.vm.pixel = [Fx::ZERO; 3];
            self.vm.pixel_written = false;
            args[0] = Value::Num(Fx::from_int(i as i32));
            if !index_only {
                // transforms apply to 2D/3D coordinates (1D x: unverifiable
                // on our oracle — its installed map can never be removed;
                // keeping 1D raw)
                let c = self.vm.pixel_coords(i, [mid; 3]);
                let c = match sel {
                    Some(s) => select_coords(c, s, mid),
                    None => c,
                };
                let p = match render {
                    RenderKind::R1(_) => c,
                    _ => self.vm.apply_transform(c),
                };
                args[1] = Value::Num(p[0]);
                args[2] = Value::Num(p[1]);
                args[3] = Value::Num(p[2]);
            }
            // One call, two implementations of the SAME entry. Everything
            // around it — coordinates, projection, brush, error policy —
            // is shared, which is what makes the two paths comparable
            // pixel for pixel (docs/jit-design.md §7).
            #[cfg(feature = "jit")]
            let outcome = match native.as_mut() {
                // SAFETY: `ctx` was built for `np` above and both are
                // still live; `args` holds four words in parameter order.
                Some((np, ctx)) => unsafe {
                    let raw = [
                        args[0].num().raw(),
                        args[1].num().raw(),
                        args[2].num().raw(),
                        args[3].num().raw(),
                    ];
                    Self::native_enter(&**np, ctx, fn_idx, &raw)
                },
                None => self.vm.render_pixel(&self.prog, &plan, &args),
            };
            #[cfg(not(feature = "jit"))]
            let outcome = self.vm.render_pixel(&self.prog, &plan, &args);
            if let Err(e) = outcome {
                let fatal = e.is_assert || e.is_resource_guard();
                if (self.last_error.is_none() || fatal) && !self.arrays_refused {
                    self.last_error = Some(e);
                }
                if fatal {
                    self.blank_from(i);
                    self.run_stage = None;
                    return;
                }
            }
            let [r, g, b] = self.vm.pixel;
            self.pixels[i as usize] = [quantize(r), quantize(g), quantize(b)];
        }
        self.project_replicate();
        self.post_chain();
        self.finish_frame();
    }

    /// Blank the frame from render call `i` on. Under a strip projection the
    /// rendered prefix has not been spread over the Layout yet, so a fatal
    /// error there blanks the whole frame rather than leaving strip colours
    /// sitting at the first `i` Layout positions.
    fn blank_from(&mut self, i: u32) {
        let from = if matches!(self.plan, ProjPlan::Strip { .. }) {
            0
        } else {
            i as usize
        };
        for p in self.pixels.iter_mut().skip(from) {
            *p = [0; 3];
        }
    }

    /// Lend the frame buffer to the VM for a `renderFrame` call. A MOVE,
    /// never a copy: at 4096 px the buffer is 12 KB and this happens every
    /// frame. The VM's bulk builtins (`crate::bulk`) write it in place and
    /// never change its length.
    fn frame_buffer_out(&mut self) {
        self.vm.frame = core::mem::take(&mut self.pixels);
    }

    /// Take it back. Paired with every [`frame_buffer_out`] on every exit
    /// path — normal return, pattern error, and debug pause.
    fn frame_buffer_in(&mut self) {
        self.pixels = core::mem::take(&mut self.vm.frame);
        debug_assert_eq!(self.pixels.len(), self.pixel_count as usize);
    }

    /// Does this pattern's code call a bulk builtin that reads a pixel's
    /// coordinate or the grid? Decides whether a `renderFrame`-only
    /// pattern gets the default square grid map.
    #[inline(never)]
    fn uses_coordinate_bulk_op(&self) -> bool {
        // Coordinate- and grid-space ops only; the index-space ones
        // (fillHSV, fade, setPixel, …) work on a bare strip and must not
        // conjure a geometry. `fillGradient`'s axis is a runtime argument,
        // so it stays out of this list — a pattern that wants a spatial
        // gradient asks for it with one of these or installs a map.
        const NAMES: [&str; 9] = [
            "gridWidth",
            "gridHeight",
            "fillRect",
            "fillCircle",
            "splat",
            "drawLine",
            "fillCanvas",
            "paintCanvas",
            "blit",
        ];
        // A fixed-length array with a never-matching sentinel: no running
        // count, no `Option` match and no subslice, all of which the
        // linker charges for in an image with 30 KB of OTA slot left.
        let mut ids = [u16::MAX; NAMES.len()];
        for (slot, name) in ids.iter_mut().zip(NAMES) {
            if let Some(id) = crate::vm::lookup_builtin(name) {
                *slot = id;
            }
        }
        crate::bytecode::calls_any_builtin(&self.prog, &ids)
    }

    /// A frame ran to completion (not a fatal-error blank, not a debug
    /// pause): hand this frame's `setPixelState` writes over as next frame's
    /// `pixelState` reads and leave the pipeline idle.
    fn finish_frame(&mut self) {
        self.vm.pixel_state_commit();
        self.run_stage = None;
    }

    /// Bytes the `pixelState` buffer holds — 0 for any pattern that never
    /// calls `setPixelState` (it is allocated on first write, not up front).
    pub fn pixel_state_bytes(&self) -> usize {
        self.vm.pixel_state_bytes()
    }

    fn render_args(&self, render: RenderKind, i: u32) -> (u16, [Value; 4], usize) {
        let mid = Fx::from_raw(1 << 15); // 0.5, mid-space fill for missing dims
                                         // transforms apply to 2D/3D coordinates (1D x: unverifiable on our
                                         // oracle — its installed map can never be removed; keeping 1D raw)
        let coords = |e: &Engine| {
            let c = e.vm.pixel_coords(i, [mid; 3]);
            match e.plan {
                ProjPlan::Select(s) => select_coords(c, s, mid),
                _ => c,
            }
        };
        let p = match render {
            // a plain render(index) never reads x: skip the per-pixel
            // coordinate divide (it is a ROM call on Xtensa) entirely
            RenderKind::R1(f) if self.prog.fns[f as usize].params < 2 => [mid; 3],
            RenderKind::R1(_) => coords(self),
            _ => self.vm.apply_transform(coords(self)),
        };
        let args = [
            Value::Num(Fx::from_int(i as i32)),
            Value::Num(p[0]),
            Value::Num(p[1]),
            Value::Num(p[2]),
        ];
        match render {
            RenderKind::R1(f) => (f, args, 2),
            RenderKind::R2(f) => (f, args, 3),
            RenderKind::R3(f) => (f, args, 4),
            RenderKind::Frame(f) => (f, args, 0), // unreachable, see render_pixels
        }
    }

    pub fn pixels(&self) -> &[[u8; 3]] {
        &self.pixels
    }

    /// The global post-process chain: whole-frame stages the engine runs
    /// once, after the last `render()` call of a frame, in a fixed order —
    ///
    ///   `setOutputPalette` → `setBlur` → `setGlow` → `setGamma`
    ///
    /// Recolor first (it works on the pattern's own luma), then spread light
    /// spatially, then apply the output transfer curve last, the way a
    /// display pipeline does. Every stage is off by default and each costs
    /// one comparison per frame when unset — an untouched pattern renders
    /// exactly as it did before the chain existed.
    ///
    /// Cost when on, per frame: the palette remap is a 3-multiply luma plus
    /// a table lookup per pixel (the 256-entry table is rebuilt only when
    /// the palette changes), blur is 6 multiply-adds per pixel per pass,
    /// glow is 3 compares plus 3 multiplies, gamma is 3 table lookups.
    /// On a grid map the two spatial stages run twice (rows, then columns)
    /// for the same per-cell cost and no extra memory — the grid is six
    /// bytes recovered once at map install, not a neighbour table.
    fn post_chain(&mut self) {
        if self.is_map {
            return;
        }
        if !self.vm.post_palette.is_empty() && self.vm.post_palette_amount != Fx::ZERO {
            self.ensure_remap_lut();
            if let Some(lut) = self.remap_lut.as_deref() {
                let amount = fx_to_256(self.vm.post_palette_amount);
                crate::outpipe::palette_remap_frame(&mut self.pixels, lut, amount);
            }
        }
        // Map-aware when the installed map is a regular grid: rows then
        // columns, so a matrix softens in 2D instead of smearing along the
        // wiring. Otherwise (strip, irregular map, no map) index space.
        let grid = self.grid.filter(|g| g.len() == self.pixels.len());
        // amount 0..1 → each neighbour's weight in 1/256ths, max 128
        let k = fx_to_256(self.vm.post_blur) / 2;
        if k > 0 {
            match &grid {
                Some(g) => crate::outpipe::blur_frame_grid(
                    &mut self.pixels,
                    g,
                    k,
                    self.vm.post_blur_passes,
                ),
                None => crate::outpipe::blur_frame(&mut self.pixels, k, self.vm.post_blur_passes),
            }
        }
        let glow = fx_to_256(self.vm.post_glow);
        if glow > 0 {
            match &grid {
                Some(g) => crate::outpipe::glow_frame_grid(&mut self.pixels, g, glow),
                None => crate::outpipe::glow_frame(&mut self.pixels, glow),
            }
        }
        self.ensure_gamma_lut();
        if let Some(lut) = self.gamma_lut.as_deref() {
            for px in self.pixels.iter_mut() {
                *px = [
                    lut[px[0] as usize],
                    lut[px[1] as usize],
                    lut[px[2] as usize],
                ];
            }
        }
    }

    /// Cook the luma → color table for the installed `setOutputPalette`
    /// stops, but only when the VM says they changed.
    fn ensure_remap_lut(&mut self) {
        let epoch = self.vm.post_palette_epoch;
        if self.remap_lut.is_some() && self.remap_lut_for == epoch {
            return;
        }
        let mut lut = alloc::boxed::Box::new([[0u8; 3]; 256]);
        crate::outpipe::fill_palette_lut(&self.vm.post_palette, &mut lut);
        self.remap_lut = Some(lut);
        self.remap_lut_for = epoch;
    }

    /// The output curve for the current `setGamma` value (rebuilt on change;
    /// gamma 0/1 disables). 255 always maps to 255.
    fn ensure_gamma_lut(&mut self) {
        let g = self.vm.post_gamma;
        if g == Fx::ZERO || g == Fx::ONE {
            self.gamma_lut = None;
            self.gamma_lut_for = g;
            return;
        }
        if self.gamma_lut.is_none() || self.gamma_lut_for != g {
            let mut lut = alloc::boxed::Box::new([0u8; 256]);
            for (i, slot) in lut.iter_mut().enumerate() {
                let v = Fx::from_raw(((i as i32) << 16) / 255);
                *slot = quantize(crate::fmath::pow(v, g));
            }
            lut[255] = 255;
            self.gamma_lut = Some(lut);
            self.gamma_lut_for = g;
        }
    }

    /// Take and clear the recorded error (hosts poll this per frame).
    pub fn take_error(&mut self) -> Option<VmError> {
        self.last_error.take()
    }

    /// Element-ledger usage and budget after init (`(used, budget)`), the
    /// pair behind the "array element budget exceeded" refusal. Reported by
    /// `luxel check` so a pattern's distance from the wall is visible at the
    /// rig it will actually run on (Gitea #420).
    pub fn array_elements(&self) -> (usize, usize) {
        (self.vm.array_elems(), self.vm.array_budget)
    }

    /// An `assert()` invariant failed during init — the pattern is blocked
    /// (renders black) until a config change rebuilds the engine. Hosts use
    /// this to pre-flight stored patterns against the current config.
    pub fn requires_violated(&self) -> bool {
        self.requires_violated
    }
}

/// Pre-flight a program's `assert()` invariants against a configuration
/// WITHOUT building a full engine: runs top-level init in a throwaway VM
/// and returns the violation message, if any. Free for assert-less
/// programs (the v4 message table makes them detectable without running
/// anything). Runtime errors that aren't asserts return None — "would
/// error" is not "declares itself incompatible", and hosts must not badge
/// patterns for OOMs caused by the pre-flight's own tighter budget.
pub fn check_asserts(prog: &Program, pixel_count: u32, array_byte_budget: usize) -> Option<String> {
    if prog.assert_msgs.is_empty() {
        return None;
    }
    let mut vm = Vm::new(prog, 1);
    vm.array_byte_budget = array_byte_budget;
    vm.globals[prog.pixel_count_g as usize] = Value::Num(Fx::from_int(pixel_count as i32));
    match vm.call(prog, 0, &[]) {
        Err(e) if e.is_assert => Some(e.message),
        _ => None,
    }
}


/// Apply a [`ProjPlan::Select`] selector: `sel[k]` names the Layout
/// coordinate feeding the pattern's k-th argument, or 3 for mid-space.
#[inline]
fn select_coords(c: [Fx; 3], sel: [u8; 3], mid: Fx) -> [Fx; 3] {
    let src = [c[0], c[1], c[2], mid];
    [
        src[(sel[0] & 3) as usize],
        src[(sel[1] & 3) as usize],
        src[(sel[2] & 3) as usize],
    ]
}

/// Fx 0..1 → 0..255 by floor(v·255) — PB-exact (pixel oracle, fw 3.67:
/// 0.5 → 127, 1−ε → 254). We used to round to nearest; floor makes whole
/// frames diff bit-identical against previewFrame captures.
pub(crate) fn quantize(v: Fx) -> u8 {
    // raw ≤ 65536 so the product fits i32: no 64-bit arithmetic per channel
    ((v.clamp(Fx::ZERO, Fx::ONE).raw() * 255) >> 16) as u8
}

/// A post-process amount (Fx 0..1) as the 0..256 integer weight the
/// `outpipe` frame stages take.
fn fx_to_256(v: Fx) -> u32 {
    ((v.raw().max(0) as u32) >> 8).min(256)
}

/// The four render entry candidates by name (`render`, `render2D`,
/// `render3D`, `renderFrame`): an exported function wins; otherwise a
/// global of that name is a late-binding candidate (see [`RenderTarget`]).
fn render_targets(prog: &Program) -> [Option<RenderTarget>; 4] {
    let tgt = |name: &str| {
        prog.exported_fn(name)
            .map(RenderTarget::Fn)
            .or_else(|| prog.global_index(name).map(RenderTarget::Global))
    };
    [
        tgt("render"),
        tgt("render2D"),
        tgt("render3D"),
        tgt("renderFrame"),
    ]
}

/// Render-function selection priority by map dimensionality (documented PB
/// behavior): no/1D map → render, render3D, render2D; 2D map → render2D,
/// render3D, render; 3D map → render3D, render2D, render. Re-run each
/// frame after `beforeRender` so a runtime-assigned entry takes effect; a
/// Global candidate resolves only while its slot holds a function.
fn resolve_render(
    tgt: &[Option<RenderTarget>; 4],
    globals: &[Value],
    dims: u8,
) -> Option<RenderKind> {
    let get = |slot: usize| {
        let f = match tgt[slot]? {
            RenderTarget::Fn(f) => f,
            RenderTarget::Global(g) => match globals.get(g as usize) {
                Some(Value::Fun(f)) => *f as u16,
                _ => return None,
            },
        };
        Some(match slot {
            0 => RenderKind::R1(f),
            1 => RenderKind::R2(f),
            2 => RenderKind::R3(f),
            _ => RenderKind::Frame(f),
        })
    };
    // `renderFrame` is not a fourth dimensionality — it is a different
    // shape of entry (one call per FRAME), so it wins over all three
    // per-pixel candidates regardless of what the map looks like.
    if let Some(f) = get(3) {
        return Some(f);
    }
    let (r1, r2, r3) = (|| get(0), || get(1), || get(2));
    match dims {
        2 => r2().or_else(r3).or_else(r1),
        3 => r3().or_else(r2).or_else(r1),
        _ => r1().or_else(r3).or_else(r2),
    }
}
