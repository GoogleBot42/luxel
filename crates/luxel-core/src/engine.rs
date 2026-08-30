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
use crate::vm::{DebugState, MapData, Outcome, Program, StepKind, Value, Vm, VmError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RenderKind {
    R1(u16),
    R2(u16),
    R3(u16),
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
    /// Entry candidates for render/render2D/render3D (fixed at load).
    render_tgt: [Option<RenderTarget>; 3],
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
    debug_enabled: bool,
    /// Some(stage) while the pipeline is suspended at a debug stop.
    run_stage: Option<RunStage>,
    cur_delta: Fx,
    /// 256-entry output curve for `setGamma` — rebuilt only when the value
    /// changes, so the per-pixel cost is one table lookup, not a pow().
    gamma_lut: Option<alloc::boxed::Box<[u8; 256]>>,
    gamma_lut_for: Fx,
    /// Map mode: this engine runs a *map program* (per-pixel `plot(x, y[, z])`)
    /// and collects coordinates instead of colors. Set via `enable_map_mode`.
    is_map: bool,
    map_coords: Vec<[Fx; 3]>,
    map_dims: u8,
    /// An `assert()` invariant failed during init: rendering is blocked
    /// for this engine's lifetime (map installs must not resurrect it —
    /// the fix is a config change, which rebuilds the engine).
    requires_violated: bool,
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
        let mut vm = Vm::new(&prog, seed);
        vm.array_byte_budget = array_byte_budget;
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
                    if let Ok(v) = vm.alloc_array(alloc::vec![Value::default(); len]) {
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

        vm.pixel_count = pixel_count;
        let before = if violated { None } else { prog.exported_fn("beforeRender") };
        let render_tgt = if violated { [None; 3] } else { render_targets(&prog) };
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
            last_error,
            debug_enabled: false,
            run_stage: None,
            cur_delta: Fx::ZERO,
            gamma_lut: None,
            gamma_lut_for: Fx::ZERO,
            is_map: false,
            map_coords: Vec::new(),
            map_dims: 0,
            requires_violated: violated,
        };

        // A pattern that renders ONLY in 2D/3D gets a default square-ish
        // grid map rather than 1D fallback coordinates. This is the
        // PB-as-experienced behavior (oracle-verified 2026-07-08): new PBs
        // ship with a default matrix map and a saved map cannot be removed
        // through the public interface, so on a real PB `render2D` always
        // receives genuine map coordinates — and the common
        // `sqrt(pixelCount)`-grid patterns depend on that. A host-installed
        // map replaces this (set_map), exactly like saving a map on a PB.
        if !violated
            && engine.render_tgt[0].is_none()
            && (engine.render_tgt[1].is_some() || engine.render_tgt[2].is_some())
        {
            engine.set_default_grid_map();
        }
        engine
    }

    /// Install the default ceil(√pixelCount)-wide row-major grid map (see
    /// from_program_budgeted for why). Public so hosts that clear a user
    /// map can fall back to the same default.
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
        let mut coords: Vec<[Fx; 3]> = Vec::new();
        if coords.try_reserve_exact(n).is_err() {
            return; // starved heap: keep the 1D fallback rather than fail
        }
        for i in 0..n {
            coords.push([
                Fx::from_int((i % w) as i32),
                Fx::from_int((i / w) as i32),
                Fx::ZERO,
            ]);
        }
        self.set_map(2, &coords);
    }

    /// Turn this engine into a *map program* runner: [`run_map`] executes its
    /// per-pixel `render(index)` (which calls `plot(x, y[, z])`) and collects
    /// one coordinate per pixel. Debugging works exactly as for a pattern —
    /// the same per-pixel `drive` loop, so breakpoints/stepping just work.
    pub fn enable_map_mode(&mut self) {
        self.is_map = true;
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
        (if self.map_dims == 0 { 2 } else { self.map_dims }, &self.map_coords)
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
            // keyed by fn-relative byte offset)
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

    /// The compiled program this engine runs (e.g. for [`crate::bytecode::serialize`]).
    pub fn program(&self) -> &Program {
        &self.prog
    }

    /// Install a pixel map: one coordinate tuple per pixel, any units.
    /// Coordinates normalize per-axis into world units 0..1 (exclusive —
    /// quantized to u16/65536 like PB's map binary). Render selection
    /// re-picks by map dimensionality.
    pub fn set_map(&mut self, dims: u8, raw: &[[Fx; 3]]) {
        let n = (self.pixel_count as usize).min(raw.len());
        let mut coords = alloc::vec![[Fx::ZERO; 3]; n];
        for axis in 0..(dims as usize).min(3) {
            let mut min = i64::MAX;
            let mut max = i64::MIN;
            for c in raw[..n].iter() {
                min = min.min(c[axis].raw() as i64);
                max = max.max(c[axis].raw() as i64);
            }
            let span = max - min;
            for (i, c) in raw[..n].iter().enumerate() {
                let v = if span == 0 {
                    0
                } else {
                    ((c[axis].raw() as i64 - min) * 65_535 + span / 2) / span
                };
                coords[i][axis] = Fx::from_raw(v as i32);
            }
        }
        self.vm.map = Some(MapData { dims, coords });
        if !self.requires_violated {
            self.render = self.resolve_render_now();
        }
    }

    /// [`resolve_render`] against the current map dims and global values.
    fn resolve_render_now(&self) -> Option<RenderKind> {
        let dims = self.vm.map.as_ref().map_or(0, |m| m.dims);
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
    pub fn var_array(&self, name: &str) -> Option<&[Value]> {
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
    ///   `beforeRender` gets the whole interval as its delta.
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
        self.frame_acc = 0;
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
                self.vm.resume(&self.prog, k)
            } else {
                match stage {
                    RunStage::Before => match self.before {
                        Some(b) => self.vm.start(
                            &self.prog,
                            b,
                            &[Value::Num(self.cur_delta)],
                            self.debug_enabled,
                        ),
                        None => Ok(Outcome::Done(Value::default())),
                    },
                    RunStage::Pixel(i) => {
                        let Some(render) = self.render else {
                            self.pixels.iter_mut().for_each(|p| *p = [0; 3]);
                            self.run_stage = None;
                            return;
                        };
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
            let outcome = match outcome {
                Err(e) => {
                    let fatal = e.is_assert || e.is_resource_guard();
                    // first error wins until read: a per-pixel error would
                    // otherwise re-alloc its message for every pixel of
                    // every frame, and the root cause is the earliest one
                    if self.last_error.is_none() || fatal {
                        self.last_error = Some(e);
                    }
                    if fatal {
                        // asserts and VM resource guards stay frame-fatal:
                        // blank the rest of the frame, move on
                        if let Some(RunStage::Pixel(i)) = self.run_stage {
                            for p in i as usize..self.pixel_count as usize {
                                self.pixels[p] = [0; 3];
                            }
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
                        // inside beforeRender) resolve now, each frame.
                        self.render = self.resolve_render_now();
                        if self.render.is_none() {
                            self.pixels.iter_mut().for_each(|p| *p = [0; 3]);
                            self.run_stage = None;
                            return;
                        }
                        if self.pixel_count == 0 {
                            self.run_stage = None;
                            return;
                        }
                        self.run_stage = Some(RunStage::Pixel(0));
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
                            let mut px = [quantize(r), quantize(g), quantize(b)];
                            if let Some(lut) = self.gamma_lut() {
                                px = [lut[px[0] as usize], lut[px[1] as usize], lut[px[2] as usize]];
                            }
                            self.pixels[i as usize] = px;
                        }
                        if i + 1 < self.pixel_count {
                            self.run_stage = Some(RunStage::Pixel(i + 1));
                        } else {
                            self.run_stage = None;
                            return;
                        }
                    }
                },
            }
        }
    }

    fn render_args(&self, render: RenderKind, i: u32) -> (u16, [Value; 4], usize) {
        let mid = Fx::from_raw(1 << 15); // 0.5, mid-space fill for missing dims
        let p = self.vm.pixel_coords(i, [mid; 3]);
        // transforms apply to 2D/3D coordinates (1D x: unverifiable on our
        // oracle — its installed map can never be removed; keeping 1D raw)
        let p = match render {
            RenderKind::R1(_) => p,
            _ => self.vm.apply_transform(p),
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
        }
    }

    pub fn pixels(&self) -> &[[u8; 3]] {
        &self.pixels
    }

    /// The output curve for the current `setGamma` value (rebuilt on change;
    /// gamma 0/1 disables). 255 always maps to 255.
    fn gamma_lut(&mut self) -> Option<&[u8; 256]> {
        let g = self.vm.post_gamma;
        if g == Fx::ZERO || g == Fx::ONE {
            self.gamma_lut = None;
            self.gamma_lut_for = g;
            return None;
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
        self.gamma_lut.as_deref()
    }

    /// Take and clear the recorded error (hosts poll this per frame).
    pub fn take_error(&mut self) -> Option<VmError> {
        self.last_error.take()
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
pub fn check_asserts(
    prog: &Program,
    pixel_count: u32,
    array_byte_budget: usize,
) -> Option<String> {
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

/// Fx 0..1 → 0..255 by floor(v·255) — PB-exact (pixel oracle, fw 3.67:
/// 0.5 → 127, 1−ε → 254). We used to round to nearest; floor makes whole
/// frames diff bit-identical against previewFrame captures.
fn quantize(v: Fx) -> u8 {
    ((v.clamp(Fx::ZERO, Fx::ONE).raw() as i64 * 255) >> 16) as u8
}

/// The three render entry candidates by name (`render`, `render2D`,
/// `render3D`): an exported function wins; otherwise a global of that name
/// is a late-binding candidate (see [`RenderTarget`]).
fn render_targets(prog: &Program) -> [Option<RenderTarget>; 3] {
    let tgt = |name: &str| {
        prog.exported_fn(name)
            .map(RenderTarget::Fn)
            .or_else(|| prog.global_index(name).map(RenderTarget::Global))
    };
    [tgt("render"), tgt("render2D"), tgt("render3D")]
}

/// Render-function selection priority by map dimensionality (documented PB
/// behavior): no/1D map → render, render3D, render2D; 2D map → render2D,
/// render3D, render; 3D map → render3D, render2D, render. Re-run each
/// frame after `beforeRender` so a runtime-assigned entry takes effect; a
/// Global candidate resolves only while its slot holds a function.
fn resolve_render(
    tgt: &[Option<RenderTarget>; 3],
    globals: &[Value],
    dims: u8,
) -> Option<RenderKind> {
    let get = |slot: usize| {
        let f = match tgt[slot]? {
            RenderTarget::Fn(f) => f,
            RenderTarget::Global(g) => match globals.get(g as usize) {
                Some(Value::Fun(f)) => *f,
                _ => return None,
            },
        };
        Some(match slot {
            0 => RenderKind::R1(f),
            1 => RenderKind::R2(f),
            _ => RenderKind::R3(f),
        })
    };
    let (r1, r2, r3) = (|| get(0), || get(1), || get(2));
    match dims {
        2 => r2().or_else(r3).or_else(r1),
        3 => r3().or_else(r2).or_else(r1),
        _ => r1().or_else(r3).or_else(r2),
    }
}
